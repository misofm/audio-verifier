use std::arch::x86_64::{__cpuid, __cpuid_count};
use std::fs;
use std::io::{self, Write as _};
use std::process;

const OSXSAVE: u32 = 1 << 27;

/// The instruction sets every supported EC2 family has, with their bits in CPUID leaf 7 EBX,
/// then ECX from bit 32.
const FEATURES: [(&str, u32); 6] = [
    ("avx512f", 16),
    ("avx512dq", 17),
    ("avx512cd", 28),
    ("avx512bw", 30),
    ("avx512vl", 31),
    ("gfni", 32 + 8),
];

/// Exits unless the CPU and the kernel support AVX-512 F, DQ, CD, BW and VL, and GFNI.
///
/// `is_x86_feature_detected!` folds to `true` for features enabled at compile time, so the
/// CPU's bits are read with CPUID, using only bit tests and writes of static bytes, which keeps
/// formatting code, itself possibly compiled with AVX-512, off that failure path. Reading XCR0
/// needs the unsafe `_xgetbv`, so kernel support for the ZMM state comes from /proc/cpuinfo,
/// where Linux lists the AVX-512 features only once it has enabled that state. If /proc/cpuinfo
/// cannot be read, only OSXSAVE is checked, and a kernel without the ZMM state faults on the
/// first AVX-512 instruction.
#[inline(never)]
pub fn require_avx512() {
    let leaf7 = if __cpuid(0).eax >= 7 {
        let leaf7 = __cpuid_count(7, 0);
        u64::from(leaf7.ebx) | u64::from(leaf7.ecx) << 32
    } else {
        0
    };
    exit_missing(b"the CPU lacks", |(_, bit)| leaf7 & (1 << bit) == 0);
    if __cpuid(1).ecx & OSXSAVE == 0 {
        let _ = io::stderr().write_all(b"error: the kernel has not enabled XSAVE\n");
        process::exit(1);
    }
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        let flags = cpuinfo
            .lines()
            .find_map(|line| line.strip_prefix("flags"))
            .unwrap_or_default();
        exit_missing(b"the kernel has not enabled", |(name, _)| {
            !flags.split_ascii_whitespace().any(|flag| flag == *name)
        });
    }
}

fn exit_missing(reason: &[u8], missing: impl Fn(&(&str, u32)) -> bool) {
    if !FEATURES.iter().any(&missing) {
        return;
    }
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(b"error: ");
    let _ = stderr.write_all(reason);
    for feature in FEATURES.iter().filter(|feature| missing(feature)) {
        let _ = stderr.write_all(b" ");
        let _ = stderr.write_all(feature.0.as_bytes());
    }
    let _ = stderr.write_all(b", which the enclave requires\n");
    process::exit(1);
}
