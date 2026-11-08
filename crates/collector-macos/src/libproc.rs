//! macOS libproc socket enumeration (`PROC_PIDLISTFDS` / `PROC_PIDFDSOCKETINFO`).

use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use common::{Direction, IpAddress, Protocol};
use libc::{c_int, c_void};

use crate::snapshot::{SocketObservation, SocketSnapshotSource, SnapshotError};

const PROC_ALL_PIDS: u32 = 1;
const PROC_PIDLISTFDS: c_int = 1;
const PROC_PIDFDSOCKETINFO: c_int = 3;
const PROX_FDTYPE_SOCKET: u32 = 2;

const SOCK_STREAM: i32 = 1;
const SOCK_DGRAM: i32 = 2;
const AF_INET: i32 = 2;
const AF_INET6: i32 = 30;
const IPPROTO_TCP: i32 = 6;
const IPPROTO_UDP: i32 = 17;

const SOCKINFO_IN: i32 = 1;
const SOCKINFO_TCP: i32 = 2;

const INI_IPV4: u8 = 0x1;
const INI_IPV6: u8 = 0x2;

const TSI_S_LISTEN: i32 = 1;
const TSI_S_CLOSED: i32 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcFdInfo {
    proc_fd: i32,
    proc_fdtype: u32,
}

#[repr(C)]
struct ProcFileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

#[repr(C)]
struct VinfoStat {
    _opaque: [u8; 136],
}

#[repr(C)]
struct SockbufInfo {
    _opaque: [u8; 24],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct In4In6Addr {
    i46a_pad32: [u32; 3],
    i46a_addr4: u32, // struct in_addr as network-order u32
}

#[repr(C)]
#[derive(Clone, Copy)]
union InAddrUnion {
    ina_46: In4In6Addr,
    ina_6: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InSockInfo {
    insi_fport: i32,
    insi_lport: i32,
    insi_gencnt: u64,
    insi_flags: u32,
    insi_flow: u32,
    insi_vflag: u8,
    insi_ip_ttl: u8,
    _pad0: u16,
    rfu_1: u32,
    insi_faddr: InAddrUnion,
    insi_laddr: InAddrUnion,
    // trailing v4/v6 protocol-dependent fields are unused by the collector.
    _tail: [u8; 16],
}

const _: () = assert!(std::mem::size_of::<InSockInfo>() == 80);

#[repr(C)]
struct SocketInfo {
    soi_stat: VinfoStat,
    soi_so: u64,
    soi_pcb: u64,
    soi_type: i32,
    soi_protocol: i32,
    soi_family: i32,
    soi_options: i16,
    soi_linger: i16,
    soi_state: i16,
    soi_qlen: i16,
    soi_incqlen: i16,
    soi_qlimit: i16,
    soi_timeo: i16,
    soi_error: u16,
    soi_oobmark: u32,
    soi_rcv: SockbufInfo,
    soi_snd: SockbufInfo,
    soi_kind: i32,
    rfu_1: u32,
    // union; TCP/UDP both place `in_sockinfo` at offset 0.
    soi_proto: [u8; 528],
}

const _: () = assert!(std::mem::size_of::<SocketInfo>() == 768);

#[repr(C)]
struct SocketFdInfo {
    pfi: ProcFileInfo,
    psi: SocketInfo,
}

const _: () = assert!(std::mem::size_of::<SocketFdInfo>() == 792);
const _: () = assert!(std::mem::size_of::<ProcFileInfo>() == 24);

#[link(name = "proc")]
unsafe extern "C" {
    fn proc_listpids(type_: u32, typeinfo: u32, buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        arg: u64,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;
    fn proc_pidfdinfo(
        pid: c_int,
        fd: c_int,
        flavor: c_int,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;
}

/// Snapshot source backed by libproc process / FD enumeration.
#[derive(Debug, Default, Clone, Copy)]
pub struct LibprocSnapshotSource;

impl SocketSnapshotSource for LibprocSnapshotSource {
    fn snapshot(&self) -> Result<Vec<SocketObservation>, SnapshotError> {
        let pids = list_pids().map_err(SnapshotError::new)?;
        let mut out = Vec::new();
        for pid in pids {
            if pid <= 0 {
                continue;
            }
            let fds = match list_fds(pid) {
                Ok(fds) => fds,
                Err(_) => continue,
            };
            for fd in fds {
                if fd.proc_fdtype != PROX_FDTYPE_SOCKET {
                    continue;
                }
                if let Some(obs) = read_socket(pid as u32, fd.proc_fd) {
                    out.push(obs);
                }
            }
        }
        Ok(out)
    }
}

fn list_pids() -> Result<Vec<i32>, String> {
    // First call with NULL to learn the required buffer size (bytes).
    // SAFETY: null buffer with size 0 is the documented sizing probe.
    let mut size = unsafe { proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if size <= 0 {
        return Err("proc_listpids sizing failed".into());
    }
    // Give headroom in case the process table grows between calls.
    size = size.saturating_mul(2);
    let count = (size as usize) / std::mem::size_of::<i32>();
    let mut buf = vec![0i32; count];
    // SAFETY: buffer is writable and sized in bytes for proc_listpids.
    let written = unsafe {
        proc_listpids(
            PROC_ALL_PIDS,
            0,
            buf.as_mut_ptr().cast(),
            (buf.len() * std::mem::size_of::<i32>()) as c_int,
        )
    };
    if written <= 0 {
        return Err("proc_listpids failed".into());
    }
    let n = (written as usize) / std::mem::size_of::<i32>();
    buf.truncate(n);
    Ok(buf)
}

fn list_fds(pid: i32) -> Result<Vec<ProcFdInfo>, String> {
    // SAFETY: sizing probe with null buffer.
    let mut size = unsafe { proc_pidinfo(pid, PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if size <= 0 {
        return Err("proc_pidinfo LISTFDS sizing failed".into());
    }
    size = size.saturating_mul(2);
    let count = (size as usize) / std::mem::size_of::<ProcFdInfo>();
    let mut buf = vec![
        ProcFdInfo {
            proc_fd: 0,
            proc_fdtype: 0,
        };
        count
    ];
    // SAFETY: buffer holds ProcFdInfo slots for PROC_PIDLISTFDS.
    let written = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDLISTFDS,
            0,
            buf.as_mut_ptr().cast(),
            (buf.len() * std::mem::size_of::<ProcFdInfo>()) as c_int,
        )
    };
    if written <= 0 {
        return Err("proc_pidinfo LISTFDS failed".into());
    }
    let n = (written as usize) / std::mem::size_of::<ProcFdInfo>();
    buf.truncate(n);
    Ok(buf)
}

fn read_socket(pid: u32, fd: i32) -> Option<SocketObservation> {
    let mut info = MaybeUninit::<SocketFdInfo>::uninit();
    let size = std::mem::size_of::<SocketFdInfo>() as c_int;
    // SAFETY: buffer is SocketFdInfo-sized; flavor is PROC_PIDFDSOCKETINFO.
    let written = unsafe {
        proc_pidfdinfo(
            pid as c_int,
            fd,
            PROC_PIDFDSOCKETINFO,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written <= 0 || written as usize != std::mem::size_of::<SocketFdInfo>() {
        return None;
    }
    // SAFETY: kernel reported a full SocketFdInfo write.
    let info = unsafe { info.assume_init() };
    observation_from_socket(pid, &info.psi)
}

fn observation_from_socket(pid: u32, soi: &SocketInfo) -> Option<SocketObservation> {
    if soi.soi_family != AF_INET && soi.soi_family != AF_INET6 {
        return None;
    }

    let protocol = match (soi.soi_type, soi.soi_protocol, soi.soi_kind) {
        (SOCK_STREAM, _, SOCKINFO_TCP) | (_, IPPROTO_TCP, SOCKINFO_TCP) => Protocol::Tcp,
        (SOCK_DGRAM, _, SOCKINFO_IN) | (_, IPPROTO_UDP, SOCKINFO_IN) => Protocol::Udp,
        (SOCK_STREAM, IPPROTO_TCP, _) => Protocol::Tcp,
        (SOCK_DGRAM, IPPROTO_UDP, _) => Protocol::Udp,
        _ => return None,
    };

    let (ini, tcp_state) = match soi.soi_kind {
        SOCKINFO_TCP => {
            // SAFETY: soi_proto starts with tcp_sockinfo / in_sockinfo for TCP.
            let ini = unsafe { read_in_sockinfo(soi.soi_proto.as_ptr()) };
            let state = unsafe {
                std::ptr::read_unaligned(soi.soi_proto.as_ptr().add(80).cast::<i32>())
            };
            (ini, Some(state))
        }
        SOCKINFO_IN => {
            let ini = unsafe { read_in_sockinfo(soi.soi_proto.as_ptr()) };
            (ini, None)
        }
        _ => return None,
    };

    if let Some(state) = tcp_state {
        if state == TSI_S_LISTEN || state == TSI_S_CLOSED {
            return None;
        }
    }

    let local_ip = ip_from_insi(&ini, true)?;
    let remote_ip = ip_from_insi(&ini, false)?;
    let local_port = ntohs_port(ini.insi_lport);
    let remote_port = ntohs_port(ini.insi_fport);

    // Skip listeners / unconnected sockets (no peer).
    if is_unspecified(remote_ip) && remote_port == 0 {
        return None;
    }

    let direction = infer_direction(local_port, remote_port);

    Some(SocketObservation {
        pid,
        protocol,
        local_ip: IpAddress::new(local_ip),
        local_port,
        remote_ip: IpAddress::new(remote_ip),
        remote_port,
        direction,
    })
}

/// SAFETY: `ptr` must point at a valid `in_sockinfo` (80 bytes).
unsafe fn read_in_sockinfo(ptr: *const u8) -> InSockInfo {
    unsafe { std::ptr::read_unaligned(ptr.cast::<InSockInfo>()) }
}

fn ip_from_insi(ini: &InSockInfo, local: bool) -> Option<IpAddr> {
    let addr = if local {
        &ini.insi_laddr
    } else {
        &ini.insi_faddr
    };
    if ini.insi_vflag & INI_IPV4 != 0 {
        // SAFETY: IPv4 branch of the address union.
        let v4 = unsafe { addr.ina_46.i46a_addr4 };
        Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(v4))))
    } else if ini.insi_vflag & INI_IPV6 != 0 {
        // SAFETY: IPv6 branch of the address union.
        let bytes = unsafe { addr.ina_6 };
        Some(IpAddr::V6(Ipv6Addr::from(bytes)))
    } else if ini.insi_vflag == 0 {
        // Some sockets leave vflag unset; try IPv4 if family was AF_INET-like.
        let v4 = unsafe { addr.ina_46.i46a_addr4 };
        Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(v4))))
    } else {
        None
    }
}

fn ntohs_port(port: i32) -> u16 {
    u16::from_be(port as u16)
}

fn is_unspecified(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_unspecified(),
        IpAddr::V6(v6) => v6.is_unspecified(),
    }
}

fn infer_direction(local_port: u16, remote_port: u16) -> Direction {
    match (local_port < 1024, remote_port < 1024) {
        (true, false) => Direction::Inbound,
        (false, true) => Direction::Outbound,
        _ => Direction::Unknown,
    }
}
