fn main() {
    embed_resource::compile("assets/brand/app.rc", embed_resource::NONE).manifest_optional().unwrap();
}
