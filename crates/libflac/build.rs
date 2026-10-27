//! Compiles libFLAC's stream decoder from `flac-1.5.0/`, vendored by
//! `scripts/vendor-libflac.sh`. The configuration is passed as defines rather
//! than through a generated `config.h`, so the build needs only a C compiler.

use std::env;

const SOURCES: [&str; 10] = [
    "bitmath",
    "bitreader",
    "cpu",
    "crc",
    "fixed",
    "format",
    "lpc",
    "md5",
    "memory",
    "stream_decoder",
];

fn main() {
    let root = "flac-1.5.0";
    let mut build = cc::Build::new();
    build
        .include(format!("{root}/include"))
        .include(format!("{root}/src/libFLAC/include"))
        .define("FLAC__NO_DLL", None)
        .define("FLAC__HAS_OGG", "0")
        .define("ENABLE_64_BIT_WORDS", "1")
        .define("HAVE_STDINT_H", "1")
        .define("HAVE_INTTYPES_H", "1")
        .define("HAVE_LROUND", "1")
        .define("HAVE_BSWAP16", "1")
        .define("HAVE_BSWAP32", "1")
        .define("PACKAGE_VERSION", "\"1.5.0\"")
        .define("NDEBUG", None)
        .opt_level(3)
        .warnings(false);

    let big_endian = env::var("CARGO_CFG_TARGET_ENDIAN").as_deref() == Ok("big");
    let flag = |set: bool| if set { "1" } else { "0" };
    build
        .define("CPU_IS_BIG_ENDIAN", flag(big_endian))
        .define("CPU_IS_LITTLE_ENDIAN", flag(!big_endian))
        .define("WORDS_BIGENDIAN", flag(big_endian));

    // x86-64 gets libFLAC's runtime CPU detection, which selects the BMI2 Rice
    // decoder when the host has it, even under a generic `-C target-cpu`.
    // Other targets build the portable C paths.
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64") {
        build
            .define("FLAC__CPU_X86_64", "1")
            .define("FLAC__HAS_X86INTRIN", "1")
            .define("FLAC__ALIGN_MALLOC_DATA", "1")
            .define("HAVE_CPUID_H", "1");
    } else {
        build.define("FLAC__NO_ASM", None);
    }

    for source in SOURCES {
        build.file(format!("{root}/src/libFLAC/{source}.c"));
    }
    build.file("src/layout.c");
    build.compile("FLAC");

    println!("cargo:rerun-if-changed={root}");
    println!("cargo:rerun-if-changed=src/layout.c");
}
