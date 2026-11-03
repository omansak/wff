fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    slint_build::compile("ui/app.slint").expect("failed to compile ui/app.slint");
    embed_resource::compile("app.rc", embed_resource::NONE).manifest_required().expect("failed to embed app.rc");
}
