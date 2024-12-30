use std::fs;

/// AOSC OS specific architecture mapping for ppc64
#[cfg(target_arch = "powerpc64")]
#[inline]
pub fn get_arch_name() -> Option<&'static str> {
    let mut endian: libc::c_int = -1;
    let result;
    unsafe {
        result = libc::prctl(libc::PR_GET_ENDIAN, &mut endian as *mut libc::c_int);
    }
    if result < 0 {
        return None;
    }
    match endian {
        libc::PR_ENDIAN_LITTLE | libc::PR_ENDIAN_PPC_LITTLE => Some("ppc64el"),
        libc::PR_ENDIAN_BIG => Some("ppc64"),
        _ => None,
    }
}

/// AOSC OS specific architecture mapping table
#[cfg(not(target_arch = "powerpc64"))]
#[inline]
pub fn get_arch_name() -> Option<&'static str> {
    use std::env::consts::ARCH;
    match ARCH {
        "x86_64" => Some("amd64"),
        "x86" => Some("i486"),
        "powerpc" => Some("powerpc"),
        "aarch64" => Some("arm64"),
        "mips64" => Some("loongson3"),
        "riscv64" => Some("riscv64"),
        "loongarch64" => Some("loongarch64"),
        _ => None,
    }
}

pub enum AOSCBranch {
    Mainline,
    Afterglow,
}

pub fn aosc_branch() -> Option<AOSCBranch> {
    let f = fs::read_to_string("/etc/os-release").ok()?;
    let lines = f.lines();
    for line in lines {
        if let Some(os) = line.strip_prefix("NAME=") {
            return match os {
                "AOSC OS" => Some(AOSCBranch::Mainline),
                "AOSC OS/Retro" | "Afterglow" => Some(AOSCBranch::Afterglow),
                _ => None,
            };
        }
    }

    None
}
