#[cfg(not(any(target_arch = "powerpc64", target_arch = "loongarch64")))]
use std::env::consts::ARCH;

use std::fs;

/// AOSC OS specific architecture mapping table
#[inline]
pub fn get_arch_name() -> Option<&'static str> {
    #[cfg(not(any(target_arch = "powerpc64", target_arch = "loongarch64")))]
    match ARCH {
        "x86_64" => Some("amd64"),
        "x86" => Some("i486"),
        "powerpc" => Some("powerpc"),
        "aarch64" => Some("arm64"),
        "mips64" => Some("loongson3"),
        "riscv64" => Some("riscv64"),
        _ => None,
    }

    #[cfg(target_arch = "powerpc64")]
    {
        let mut endian: libc::c_int = -1;
        let result = unsafe { libc::prctl(libc::PR_GET_ENDIAN, &mut endian as *mut libc::c_int) };
        if result < 0 {
            return None;
        }
        match endian {
            libc::PR_ENDIAN_LITTLE | libc::PR_ENDIAN_PPC_LITTLE => Some("ppc64el"),
            libc::PR_ENDIAN_BIG => Some("ppc64"),
            _ => None,
        }
    }

    #[cfg(target_arch = "loongarch64")]
    {
        use core::arch::asm;

        fn loongarch_has_simd() -> bool {
            let mask: u64;
            const CONFIG_PAGE: u64 = 2;
            unsafe {
                asm! {
                    "cpucfg {mask}, {page}",
                    options(pure, nomem, nostack, preserves_flags),
                    page = in(reg) CONFIG_PAGE,
                    mask = lateout(reg) mask,
                }
                (mask >> 6) & 0x1 > 0
            }
        }

        if loongarch_has_simd() {
            Some("loongarch64")
        } else {
            Some("loongarch64_nosimd")
        }
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
