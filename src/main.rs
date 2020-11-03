#[macro_use]
extern crate clap;

use clap::App;
use glob::glob;
use std::{collections, convert::TryInto, fs, io::Write, path};

#[derive(Debug, Clone, Copy)]
struct PackedOffset(u64, u64);
#[derive(Debug)]
struct PackedFile {
    pointers: Vec<PackedOffset>,
    path: path::PathBuf,
    // last_modified: &'a std::time::SystemTime,
    byte_done: u64,
    byte_size: u64,
    bytes: Vec<u8>,
    done: bool,
    slice: u64,
}

#[derive(Debug)]
struct ArchivedFile {
    pointers: Vec<PackedOffset>,
    path: path::PathBuf, //Might be useless
                         // last_modified: &'a std::time::SystemTime,
}
#[derive(Debug)]
struct PackedFileHeader(collections::HashMap<path::PathBuf, ArchivedFile>);
#[derive(Debug)]
struct PackedArchive {
    header: PackedFileHeader,
    bytes: Vec<u8>,
}

fn main() {
    let yaml = load_yaml!(r#"cli.yaml"#);
    let matches = App::from_yaml(yaml).get_matches();
    if let Some(matches) = matches.subcommand_matches("compress") {
        for val in matches.values_of("FOLDER").unwrap() {
            let archive = walk_folder(val);
            let packed_archive = compress_archive(&mut archive.unwrap());
            write_to_file(packed_archive, &format!("{}.ls", val)).unwrap();
        }
    }
    if let Some(matches) = matches.subcommand_matches("decompress") {
        for val in matches.values_of("FILE").unwrap() {
            decompress_file(val);
        }
    }
}

fn zstd_file(bytes: &mut [u8]) {}

fn walk_folder(folder: &str) -> std::io::Result<Vec<PackedFile>> {
    let mut archive = Vec::<PackedFile>::new();
    for entry in glob(format!("{}/**/*", folder).as_str())
        .unwrap()
        .filter_map(Result::ok)
    {
        let is_dir = fs::metadata(&entry).unwrap().is_dir();
        let byte_size = fs::metadata(&entry).unwrap().len();
        if !is_dir {
            let mut bytes = fs::read(&entry).unwrap();
            zstd_file(&mut bytes);
            let file = PackedFile {
                pointers: Vec::<PackedOffset>::new(),
                path: entry
                    .clone()
                    .strip_prefix(format!("{}", folder))
                    .unwrap()
                    .to_path_buf(),
                byte_done: 0,
                byte_size,
                bytes,
                done: false,
                slice: 10u32.pow((byte_size as f64).log10().ceil() as u32 - 1) as u64,
            };
            archive.push(file);
        }
    }
    Ok(archive)
}

fn compress_archive(packed_file: &mut Vec<PackedFile>) -> PackedArchive {
    let mut offset = 0;
    let mut archive = PackedArchive {
        header: PackedFileHeader {
            0: collections::HashMap::<path::PathBuf, ArchivedFile>::new(),
        },
        bytes: Vec::<u8>::new(),
    };
    let mut done_count = 0;
    let to_do = packed_file.len();
    while done_count < to_do {
        for mut file in &mut *packed_file {
            if !file.done {
                let bytes_to_read = std::cmp::min(file.byte_size - file.byte_done, file.slice);

                //Add byte offsets
                let new_offset = PackedOffset(offset, offset + (bytes_to_read - 1));
                file.pointers.push(new_offset);
                offset += bytes_to_read;
                archive.bytes.extend_from_slice(
                    &file.bytes[file.byte_done as usize..(file.byte_done + bytes_to_read) as usize],
                );

                file.byte_done += bytes_to_read;
                if file.byte_done == file.byte_size {
                    archive.header.0.insert(
                        file.path.clone(),
                        ArchivedFile {
                            pointers: file.pointers.clone(),
                            path: file.path.clone(),
                        },
                    );
                    file.done = true;
                    done_count += 1;
                }
            }
        }
    }

    return archive;
}

fn write_to_file(archive: PackedArchive, file_name: &String) -> std::io::Result<()> {
    let mut bytes_to_write = Vec::<u8>::new();

    // Version
    let version = b"LSArc1";
    bytes_to_write.extend_from_slice(version);

    // TODO ENCRYPT ARCHIVE HERE
    for (key, value) in archive.header.0.iter() {
        // Path name
        let key_string_bytes = key.clone().into_os_string().into_string().unwrap();
        let key_string_bytes = key_string_bytes.as_bytes();
        bytes_to_write.extend_from_slice(key_string_bytes);
        bytes_to_write.push(0);

        //Pointers
        let n_pointers: u8 = value.pointers.len() as u8;
        bytes_to_write.push(n_pointers);
        for pointer in &value.pointers {
            let start = pointer.0;
            let end = pointer.1;
            bytes_to_write.extend_from_slice(&start.to_be_bytes());
            bytes_to_write.extend_from_slice(&end.to_be_bytes());
        }
    }

    let mut file = std::fs::File::create(file_name)?;
    let offset = bytes_to_write.len() as u32 + 4;
    file.write_all(&offset.to_be_bytes())?;
    bytes_to_write.extend_from_slice(&archive.bytes);
    file.write_all(&bytes_to_write)?;
    Ok(())
}

fn decompress_file(file: &str) {
    let bytes = fs::read(file).unwrap();
    let offset = u32::from_be_bytes(bytes[0..4].try_into().expect("Unable to read offset."));
    println!("{}", offset);

    let header_bytes = &bytes[4..offset as usize];

    let mut cursor: usize = 10;
    while cursor < offset as usize {
        let old_cursor = cursor;
        while &[bytes[cursor]] != b"\0" {
            cursor += 1;
        }
        let mut path_bytes = String::from_utf8_lossy(&bytes[old_cursor..cursor]);
        println!("{}", path_bytes);

        cursor += 1;
        let n_pointers = bytes[cursor];
        cursor += 1;

        let mut data = Vec::<u8>::new();
        for _ in 1..=n_pointers {
            let pointer_start = u64::from_be_bytes(
                bytes[cursor..cursor + 8]
                    .try_into()
                    .expect("unable to read bytes"),
            ) as usize;
            let pointer_end = u64::from_be_bytes(
                bytes[cursor + 8..cursor + 16]
                    .try_into()
                    .expect("unable to read bytes"),
            ) as usize;
            let pointer_start = pointer_start + offset as usize;
            let pointer_end = pointer_end + offset as usize;
            println!("{}:{},{}", cursor, pointer_start, pointer_end);
            cursor += 16;
            data.extend_from_slice(&bytes[pointer_start..=pointer_end]);
        }

        // TODO DECRYPT here
        let path = format!("extract/{}/{}", file, path_bytes);
        let path = std::path::Path::new(&path);
        let prefix = path.parent().unwrap();
        std::fs::create_dir_all(prefix).expect("error creating directory");
        let mut file = std::fs::File::create(&path).expect("error creating file");
        file.write_all(data.as_slice()).expect("error writing file");
    }
}
