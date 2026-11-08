//! macOS process lookup via `libproc`.

use std::ffi::CStr;
use std::mem::MaybeUninit;

use chrono::{TimeZone, Utc};
use libc::{c_char, c_int, c_void, gid_t, uid_t};

use crate::source::{ProcessInfo, ProcessSource};

const PROC_PIDTBSDINFO: c_int = 3;
const PROC_PIDPATHINFO_MAXSIZE: usize = 4 * 1024;
const MAXCOMLEN: usize = 16;

#[repr(C)]
struct ProcBsdInfo {
    pbi_flags: u32,
    pbi_status: u32,
    pbi_xstatus: u32,
    pbi_pid: u32,
    pbi_ppid: u32,
    pbi_uid: uid_t,
    pbi_gid: gid_t,
    pbi_ruid: uid_t,
    pbi_rgid: gid_t,
    pbi_svuid: uid_t,
    pbi_svgid: gid_t,
    rfu_1: u32,
    pbi_comm: [c_char; MAXCOMLEN],
    pbi_name: [c_char; 2 * MAXCOMLEN],
    pbi_nfiles: u32,
    pbi_pgid: u32,
    pbi_pjobc: u32,
    e_tdev: u32,
    e_tpgid: u32,
    pbi_nice: i32,
    pbi_start_tvsec: u64,
    pbi_start_tvusec: u64,
}

#[link(name = "proc")]
unsafe extern "C" {
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        arg: u64,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;

    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;

    fn proc_name(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
}

/// macOS [`ProcessSource`] using `libproc` (`proc_pidinfo`, `proc_pidpath`, `proc_name`).
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsProcessSource;

impl ProcessSource for MacOsProcessSource {
    fn info(&self, pid: u32) -> ProcessInfo {
        let bsd = read_bsd_info(pid);
        let executable_path = read_pid_path(pid);
        let name = bsd
            .as_ref()
            .and_then(name_from_bsd)
            .or_else(|| read_proc_name(pid));
        let process_start_time = bsd.as_ref().and_then(start_time_from_bsd);

        ProcessInfo {
            pid,
            name,
            executable_path,
            process_start_time,
        }
    }
}

fn read_bsd_info(pid: u32) -> Option<ProcBsdInfo> {
    let mut info = MaybeUninit::<ProcBsdInfo>::uninit();
    let size = std::mem::size_of::<ProcBsdInfo>() as c_int;
    // SAFETY: buffer points to a ProcBsdInfo-sized allocation; flavor matches layout.
    let written = unsafe {
        proc_pidinfo(
            pid as c_int,
            PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written <= 0 || written as usize != std::mem::size_of::<ProcBsdInfo>() {
        return None;
    }
    // SAFETY: proc_pidinfo reported a full ProcBsdInfo write.
    Some(unsafe { info.assume_init() })
}

fn read_pid_path(pid: u32) -> Option<String> {
    let mut buf = vec![0u8; PROC_PIDPATHINFO_MAXSIZE];
    // SAFETY: buffer is writable and sized to PROC_PIDPATHINFO_MAXSIZE.
    let written = unsafe {
        proc_pidpath(
            pid as c_int,
            buf.as_mut_ptr().cast(),
            PROC_PIDPATHINFO_MAXSIZE as u32,
        )
    };
    if written <= 0 {
        return None;
    }
    c_buf_to_string(&buf[..written as usize])
}

fn read_proc_name(pid: u32) -> Option<String> {
    let mut buf = [0u8; 2 * MAXCOMLEN];
    // SAFETY: buffer is writable; proc_name writes a NUL-terminated name.
    let written =
        unsafe { proc_name(pid as c_int, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if written <= 0 {
        return None;
    }
    c_buf_to_string(&buf)
}

fn name_from_bsd(info: &ProcBsdInfo) -> Option<String> {
    c_char_array_to_string(&info.pbi_name).or_else(|| c_char_array_to_string(&info.pbi_comm))
}

fn start_time_from_bsd(info: &ProcBsdInfo) -> Option<chrono::DateTime<Utc>> {
    let secs = i64::try_from(info.pbi_start_tvsec).ok()?;
    let micros = info.pbi_start_tvusec.min(1_999_999) as u32;
    let nanos = micros.checked_mul(1_000)?;
    Utc.timestamp_opt(secs, nanos).single()
}

fn c_char_array_to_string(buf: &[c_char]) -> Option<String> {
    if buf.is_empty() || buf[0] == 0 {
        return None;
    }
    // SAFETY: bytes originate from a kernel C string field; stop at first NUL.
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len()) };
    c_buf_to_string(bytes)
}

fn c_buf_to_string(buf: &[u8]) -> Option<String> {
    let cstr = CStr::from_bytes_until_nul(buf).ok()?;
    let s = cstr.to_str().ok()?.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
}
