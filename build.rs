extern crate embed_resource;

fn main() {
    let _ = embed_resource::compile("app_icon.rc", embed_resource::NONE);
}
