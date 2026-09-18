//! The exe's own icon.
//!
//! The window icon is set at runtime from `images/chinchou.png`, but Explorer,
//! the taskbar and the alt-tab list read an icon compiled into the binary. That
//! has to be an `.ico`, so one is built from the same png rather than kept
//! beside it as a second file that can drift.

fn main() {
    println!("cargo:rerun-if-changed=images/chinchou.png");

    #[cfg(windows)]
    {
        match icon() {
            Ok(path) => {
                let mut resource = winresource::WindowsResource::new();
                resource.set_icon(&path.to_string_lossy());
                if let Err(error) = resource.compile() {
                    println!("cargo:warning=icon embed failed: {error}");
                }
            }
            Err(error) => println!("cargo:warning=icon build failed: {error}"),
        }
    }
}

/// Write `chinchou.ico` into the build folder: the png scaled to every size
/// Windows asks for, largest first.
#[cfg(windows)]
fn icon() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    const SIZES: [u32; 6] = [256, 128, 64, 48, 32, 16];

    let source = image::open("images/chinchou.png")?.to_rgba8();
    let mut set = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let scaled = image::imageops::resize(
            &source,
            size,
            size,
            image::imageops::FilterType::Lanczos3,
        );
        let frame = ico::IconImage::from_rgba_data(size, size, scaled.into_raw());
        set.add_entry(ico::IconDirEntry::encode(&frame)?);
    }

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("chinchou.ico");
    set.write(std::io::BufWriter::new(std::fs::File::create(&out)?))?;
    Ok(out)
}
