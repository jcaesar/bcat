#![cfg_attr(target_os = "wasi", feature(wasi_ext))]

cfg_if::cfg_if! {
    if #[cfg(target_os = "wasi")] {
        #[macro_use]
        extern crate prettytable_rs_wasi;
        use prettytable_rs_wasi::format;
        use prettytable_rs_wasi::Table;
    } else {
        #[macro_use]
        extern crate prettytable;
        use prettytable::format;
        use prettytable::Table;
    }
}

use anyhow::Result;
use chrono::{Local, TimeZone};
use filesize::PathExt;
use humansize::{file_size_opts as options, FileSize};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process;
use structopt::StructOpt;
cfg_if::cfg_if! {
    if #[cfg(unix)] {
        fn get_user_by_uid(u: u32) -> String {
            users::get_user_by_uid(u)
                .map(|u| u.name().to_str().unwrap_or_default().to_owned())
                .unwrap_or_default()
        }
        fn get_group_by_gid(u: u32) -> String {
            users::get_group_by_gid(u)
                .map(|g| g.name().to_str().unwrap_or_default().to_owned())
                .unwrap_or_default()
        }
        use std::os::unix::fs::MetadataExt;
    } else {
        fn get_user_by_uid(_: u32) -> String { "".into() }
        fn get_group_by_gid(_: u32) -> String { "".into() }
        trait BCatMetadataExt {
            fn mtime(&self) -> i64;
            fn uid(&self) -> u32;
            fn gid(&self) -> u32;
            fn mode(&self) -> u32;
        }
        impl BCatMetadataExt for fs::Metadata {
            #[cfg(target_os = "wasi")]
            fn mtime(&self) -> i64 {
                use std::os::wasi::fs::MetadataExt;
                (self.mtim() / 1e9 as u64) as i64
            }
            #[cfg(windows)]
            fn mtime(&self) -> i64 {
                use std::os::windows::fs::MetadataExt;
                (self.last_write_time() / 1e7 as u64) as i64 - 11_644_473_600
            }
            #[cfg(not(any(windows, target_os = "wasi")))]
            fn mtime(&self) -> i64 { i64::MAX }
            fn uid(&self) -> u32 { u32::MAX }
            fn gid(&self) -> u32 { u32::MAX }
            fn mode(&self) -> u32 { 0 }
        }
    }
}

#[derive(StructOpt)]
struct Cli {
    path: PathBuf,
}

const SIZE_LESS: u64 = 1024 * 10;

fn main() -> Result<()> {
    let args = Cli::from_args();

    let path = fs::canonicalize(&args.path).unwrap_or(args.path);

    let mut file = fs::File::open(&path)?;
    let metadata = file.metadata()?;

    return if metadata.is_file() {
        #[cfg(unix)]
        if SIZE_LESS < metadata.len() {
            use std::os::unix::prelude::CommandExt;
            //TODO impl less
            process::Command::new("less").arg(path).exec();
            return Ok(());
        }

        read_file(&mut file)
    } else {
        list_dir(&path)
    };
}

fn read_file(file: &mut fs::File) -> Result<()> {
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    println!("{}", buf);
    Ok(())
}

fn list_dir(path: &PathBuf) -> Result<()> {
    let mut table = Table::new();
    if cfg!(unix) {
        table.set_titles(row![
            "permission",
            "user",
            "group",
            "name",
            "last-modify",
            "size"
        ]);
    } else {
        table.set_titles(row!["name", "last-modify", "size"]);
    }
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    let mut paths: Vec<_> = fs::read_dir(path)?.map(|r| r.unwrap()).collect();

    paths.sort_by_key(|f| {
        f.file_name()
            .to_os_string()
            .to_string_lossy()
            .to_lowercase()
    });

    for entry in paths {
        let path = entry.path();
        let meta = fs::metadata(&path)?;
        let uid = meta.uid();
        let user = get_user_by_uid(uid);
        let gid = meta.gid();
        let group = get_group_by_gid(gid);
        let stat = meta.mode();
        let size = path
            .size_on_disk()?
            .file_size(options::CONVENTIONAL)
            .unwrap_or_default();
        let lmtime = Local
            .timestamp_opt(meta.mtime(), 0)
            .map(|x| x.to_string())
            .single()
            .unwrap_or_default();

        let file_name = match path.file_name() {
            Some(result) => result.to_string_lossy(),
            None => continue,
        };

        if cfg!(unix) {
            table.add_row(row![
                &unix_mode::to_string(stat),
                &user,
                &group,
                &file_name,
                &lmtime,
                &size,
            ]);
        } else {
            table.add_row(row![&file_name, &lmtime, &size,]);
        }
    }
    table.printstd();
    Ok(())
}
