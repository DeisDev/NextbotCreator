use std::{
    env,
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

fn main() {
    println!("cargo:rerun-if-changed=assets/app-icon.png");
    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let icon_path = out_dir.join("nextbot-creator.ico");
    let decoder = png::Decoder::new(BufReader::new(
        File::open("assets/app-icon.png").expect("open app icon"),
    ));
    let mut reader = decoder.read_info().expect("read app icon metadata");
    let mut bytes = vec![0; reader.output_buffer_size().expect("icon buffer size")];
    let info = reader.next_frame(&mut bytes).expect("decode app icon");
    bytes.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes,
        _ => panic!("assets/app-icon.png must be RGBA"),
    };

    let mut directory = ico::IconDir::new(ico::ResourceType::Icon);
    let image = ico::IconImage::from_rgba_data(info.width, info.height, rgba);
    directory.add_entry(ico::IconDirEntry::encode(&image).expect("encode app icon"));
    directory
        .write(BufWriter::new(
            File::create(&icon_path).expect("create app icon"),
        ))
        .expect("write app icon");

    let mut resource = winres::WindowsResource::new();
    resource
        .set_icon(icon_path.to_str().expect("UTF-8 icon path"))
        .set("FileDescription", "NextbotCreator")
        .set("ProductName", "NextbotCreator")
        .set("LegalCopyright", "GPL-3.0-only");
    resource.compile().expect("compile Windows resources");
}
