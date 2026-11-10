//! macOS libproc socket enumeration (`PROC_PIDLISTFDS` / `PROC_PIDFDSOCKETINFO`).

use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};

use common::{Direction, IpAddress, Protocol};
use libc::{c_int, c_void};

use crate::ping_args::ping_destination;
use crate::snapshot::{SnapshotError, SocketObservation, SocketSnapshotSource};

const PROC_ALL_PIDS: u32 = 1;
const PROC_PIDLISTFDS: c_int = 1;
const PROC_PIDFDSOCKETINFO: c_int = 3;
const PROX_FDTYPE_SOCKET: u32 = 2;

const SOCK_STREAM: i32 = 1;
const SOCK_DGRAM: i32 = 2;
const AF_INET: i32 = 2;
const AF_INET6: i32 = 30;
const IPPROTO_ICMP: i32 = 1;
const IPPROTO_TCP: i32 = 6;
const IPPROTO_UDP: i32 = 17;
const IPPROTO_ICMPV6: i32 = 58;

const CTL_KERN: c_int = 1;
const KERN_PROCARGS2: c_int = 49;

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

    fn proc_name(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
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

    // ICMP before the datagram wildcard: macOS ping uses SOCK_DGRAM + IPPROTO_ICMP.
    let protocol = if soi.soi_kind == SOCKINFO_IN
        && (soi.soi_protocol == IPPROTO_ICMP || soi.soi_protocol == IPPROTO_ICMPV6)
    {
        Protocol::Icmp
    } else {
        match (soi.soi_type, soi.soi_protocol, soi.soi_kind) {
            (SOCK_STREAM, _, SOCKINFO_TCP) | (_, IPPROTO_TCP, SOCKINFO_TCP) => Protocol::Tcp,
            (SOCK_DGRAM, _, SOCKINFO_IN) | (_, IPPROTO_UDP, SOCKINFO_IN) => Protocol::Udp,
            (SOCK_STREAM, IPPROTO_TCP, _) => Protocol::Tcp,
            (SOCK_DGRAM, IPPROTO_UDP, _) => Protocol::Udp,
            _ => return None,
        }
    };

    let (ini, tcp_state) = match soi.soi_kind {
        SOCKINFO_TCP => {
            // SAFETY: soi_proto starts with tcp_sockinfo / in_sockinfo for TCP.
            let ini = unsafe { read_in_sockinfo(soi.soi_proto.as_ptr()) };
            let state =
                unsafe { std::ptr::read_unaligned(soi.soi_proto.as_ptr().add(80).cast::<i32>()) };
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

    let mut local_ip = ip_from_insi(&ini, true)?;
    let mut remote_ip = ip_from_insi(&ini, false)?;
    let local_port = ntohs_port(ini.insi_lport);
    let remote_port = ntohs_port(ini.insi_fport);
    let mut remote_hostname = None;

    // ping keeps an unconnected ICMP socket and sendto()s each echo. The
    // foreign address stays 0.0.0.0; recover the target from argv.
    if protocol == Protocol::Icmp && is_unspecified(remote_ip) && remote_port == 0 {
        let (peer, local, hostname) = recover_icmp_peer(pid, soi.soi_family)?;
        remote_ip = peer;
        if !is_unspecified(local) {
            local_ip = local;
        }
        remote_hostname = hostname;
    }

    // Skip listeners / unconnected sockets (no peer).
    if is_unspecified(remote_ip) && remote_port == 0 {
        return None;
    }

    let direction = if protocol == Protocol::Icmp {
        Direction::Outbound
    } else {
        infer_direction(local_port, remote_port)
    };

    Some(SocketObservation {
        pid,
        protocol,
        local_ip: IpAddress::new(local_ip),
        local_port,
        remote_ip: IpAddress::new(remote_ip),
        remote_port,
        direction,
        remote_hostname,
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

struct IcmpPeer {
    host: String,
    ip: IpAddr,
    local_ip: IpAddr,
    hostname: Option<String>,
}

fn icmp_cache() -> &'static Mutex<HashMap<u32, IcmpPeer>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, IcmpPeer>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Remote address for an unconnected ICMP socket owned by `ping`.
///
/// Returns `(remote, local, hostname)`. `hostname` is set when the argument
/// was a name rather than a numeric address.
fn recover_icmp_peer(pid: u32, family: i32) -> Option<(IpAddr, IpAddr, Option<String>)> {
    match process_comm(pid).as_deref() {
        Some(name) if !is_ping_comm(name) => {
            if let Ok(mut cache) = icmp_cache().lock() {
                cache.remove(&pid);
            }
            return None;
        }
        Some(_) | None => {}
    }

    if let Some(args) = read_process_args(pid) {
        if let Some(host) = ping_destination(&args) {
            if let Some(peer) = peer_for_host(pid, host, family) {
                return Some(peer);
            }
        }
    }

    let cache = icmp_cache().lock().ok()?;
    let peer = cache.get(&pid)?;
    if !family_matches(peer.ip, family) {
        return None;
    }
    Some((peer.ip, peer.local_ip, peer.hostname.clone()))
}

fn peer_for_host(pid: u32, host: &str, family: i32) -> Option<(IpAddr, IpAddr, Option<String>)> {
    if let Ok(cache) = icmp_cache().lock() {
        if let Some(peer) = cache.get(&pid) {
            if peer.host == host && family_matches(peer.ip, family) {
                return Some((peer.ip, peer.local_ip, peer.hostname.clone()));
            }
        }
    }

    let ip = resolve_host(host, family)?;
    let local_ip = local_ip_toward(ip).unwrap_or_else(|| unspecified_like(ip));
    let hostname = if parse_ip_token(host).is_some() {
        None
    } else {
        Some(host.to_owned())
    };
    if let Ok(mut cache) = icmp_cache().lock() {
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(
            pid,
            IcmpPeer {
                host: host.to_owned(),
                ip,
                local_ip,
                hostname: hostname.clone(),
            },
        );
    }
    Some((ip, local_ip, hostname))
}

fn resolve_host(host: &str, family: i32) -> Option<IpAddr> {
    if let Some(ip) = parse_ip_token(host) {
        return family_matches(ip, family).then_some(ip);
    }
    let addrs = (host, 0u16).to_socket_addrs().ok()?;
    addrs
        .map(|addr| addr.ip())
        .find(|ip| family_matches(*ip, family))
}

fn parse_ip_token(token: &str) -> Option<IpAddr> {
    let trimmed = token.trim();
    let bare = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(trimmed);
    let bare = bare.split('%').next().unwrap_or(bare);
    bare.parse().ok()
}

fn family_matches(ip: IpAddr, family: i32) -> bool {
    match family {
        AF_INET => ip.is_ipv4(),
        AF_INET6 => ip.is_ipv6(),
        _ => false,
    }
}

fn unspecified_like(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    }
}

fn local_ip_toward(remote: IpAddr) -> Option<IpAddr> {
    let bind = match remote {
        IpAddr::V4(_) => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        IpAddr::V6(_) => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
    };
    let socket = std::net::UdpSocket::bind(bind).ok()?;
    // UDP connect selects a source address and does not transmit.
    socket.connect(SocketAddr::new(remote, 443)).ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

fn is_ping_comm(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    base == "ping" || base == "ping6"
}

fn process_comm(pid: u32) -> Option<String> {
    let mut buf = [0u8; 64];
    // SAFETY: buffer is writable; proc_name writes a NUL-terminated name.
    let written = unsafe { proc_name(pid as c_int, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if written <= 0 {
        return None;
    }
    let end = buf.iter().position(|byte| *byte == 0).unwrap_or(buf.len());
    let name = std::str::from_utf8(&buf[..end]).ok()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn read_process_args(pid: u32) -> Option<Vec<String>> {
    let mut mib = [CTL_KERN, KERN_PROCARGS2, pid as c_int];
    let mut size: libc::size_t = 0;
    // SAFETY: null oldp asks for the buffer size only.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || size < 4 {
        return None;
    }
    size = size.saturating_add(256);
    let mut buf = vec![0u8; size];
    let mut actual = buf.len();
    // SAFETY: buf is writable and actual tracks its length in bytes.
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buf.as_mut_ptr().cast(),
            &mut actual,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || actual < 4 || actual > buf.len() {
        return None;
    }
    buf.truncate(actual);
    let args = argv_from_procargs2(&buf);
    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

/// Decodes the `KERN_PROCARGS2` layout: argc, executable path, then argv strings.
fn argv_from_procargs2(buf: &[u8]) -> Vec<String> {
    if buf.len() < 4 {
        return Vec::new();
    }
    let argc = i32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if argc <= 0 {
        return Vec::new();
    }
    let mut index = 4usize;
    while index < buf.len() && buf[index] != 0 {
        index += 1;
    }
    while index < buf.len() && buf[index] == 0 {
        index += 1;
    }
    let mut args = Vec::with_capacity(argc as usize);
    for _ in 0..argc {
        if index >= buf.len() {
            break;
        }
        let start = index;
        while index < buf.len() && buf[index] != 0 {
            index += 1;
        }
        match std::str::from_utf8(&buf[start..index]) {
            Ok(arg) => args.push(arg.to_owned()),
            Err(_) => return args,
        }
        if index < buf.len() {
            index += 1;
        }
    }
    args
}
