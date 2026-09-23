fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../../assets/AppIcon.ico");
        embed_resource::compile("selo.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
