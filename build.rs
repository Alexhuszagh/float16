fn main() {
    let version = rustc_version::version().unwrap();
    let version_meta = rustc_version::version_meta().unwrap();
    let support_unstable = version_meta.channel == rustc_version::Channel::Nightly;

    if version >= rustc_version::Version::parse("1.63.0").unwrap() {
        println!("cargo:rustc-cfg=has_aarch64_intrinsics");
    }

    if version >= rustc_version::Version::parse("1.70.0").unwrap() {
        println!("cargo:rustc-cfg=has_x86_intrinsics");
    }

    if support_unstable && version >= rustc_version::Version::parse("1.89.0").unwrap() {
        println!("cargo:rustc-cfg=has_loongarch_intrinsics");
    }
}
