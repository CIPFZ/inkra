//! `cargo xtask dist`: release packages for the host platform (NF-006).
//!
//! - Linux: `tar.gz`, `.deb` (written directly), `.rpm` (when `rpmbuild` is
//!   installed) and an AppImage (when `appimagetool` is installed).
//! - Windows: a portable `.zip` and an `.msi` (when the WiX Toolset v3 `candle`
//!   and `light` are on `PATH` or in `WIX`).
//! - macOS: `Inkra.app` and a `.dmg` (with `hdiutil`).
//!
//! Every package registers Inkra for Markdown files and carries the brand icons
//! (UI-050).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{cargo, root, run};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Markdown extensions associated with Inkra.
const EXTS: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext",
];

fn io(e: std::io::Error) -> String {
    e.to_string()
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p).map_err(io)?;
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} → {}: {e}", from.display(), to.display()))
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success() || !o.stderr.is_empty() || !o.stdout.is_empty())
}

/// Builds the release binaries and the packages; returns the files made.
pub fn dist() -> Result<Vec<PathBuf>, String> {
    let mut build = cargo();
    build.args(["build", "--release", "-p", "inkra", "-p", "inkra-cli"]);
    run("build", build)?;
    let root = root();
    let release = root.join("target/release");
    let out = root.join("target/dist");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(io)?;
    if cfg!(windows) {
        windows(&root, &release, &out)
    } else if cfg!(target_os = "macos") {
        macos(&root, &release, &out)
    } else {
        linux(&root, &release, &out)
    }
}

fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

// ---- Linux ----

/// The installed tree under `prefix` (`usr/` for packages).
fn linux_tree(root: &Path, release: &Path, dest: &Path) -> Result<(), String> {
    for b in ["inkra", "inkra-cli"] {
        copy(&release.join(b), &dest.join("bin").join(b))?;
    }
    copy(
        &root.join("apps/inkra/linux/io.inkra.Inkra.desktop"),
        &dest.join("share/applications/io.inkra.Inkra.desktop"),
    )?;
    copy(
        &root.join("apps/inkra/linux/io.inkra.Inkra.metainfo.xml"),
        &dest.join("share/metainfo/io.inkra.Inkra.metainfo.xml"),
    )?;
    copy(
        &root.join("apps/inkra/linux/io.inkra.Inkra.xml"),
        &dest.join("share/mime/packages/io.inkra.Inkra.xml"),
    )?;
    let hicolor = root.join("assets/brand/hicolor");
    for entry in std::fs::read_dir(&hicolor).map_err(io)? {
        let size = entry.map_err(io)?.path();
        let src = size.join("apps/io.inkra.Inkra.png");
        let symbolic = size.join("apps/io.inkra.Inkra-symbolic.png");
        let name = size
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if src.is_file() {
            copy(
                &src,
                &dest
                    .join("share/icons/hicolor")
                    .join(&name)
                    .join("apps/io.inkra.Inkra.png"),
            )?;
        } else if symbolic.is_file() {
            copy(
                &symbolic,
                &dest
                    .join("share/icons/hicolor")
                    .join(&name)
                    .join("apps/io.inkra.Inkra-symbolic.png"),
            )?;
        }
    }
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "README.md"] {
        copy(&root.join(f), &dest.join("share/doc/inkra").join(f))?;
    }
    Ok(())
}

fn tar_gz(dir: &Path, name: &str, out: &Path) -> Result<PathBuf, String> {
    let file = out.join(format!("{name}.tar.gz"));
    let mut c = Command::new("tar");
    c.arg("-C").arg(dir).arg("-czf").arg(&file).arg(name);
    run("tar", c)?;
    Ok(file)
}

/// An `ar` archive (the `.deb` container).
fn ar(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for (name, data) in members {
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            name,
            0,
            0,
            0,
            "100644",
            data.len()
        );
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(b'\n');
        }
    }
    out
}

fn deb(staging: &Path, out: &Path) -> Result<PathBuf, String> {
    let work = staging.join("deb");
    let data = work.join("data");
    let control = work.join("control");
    std::fs::create_dir_all(&control).map_err(io)?;
    linux_tree(&root(), &root().join("target/release"), &data.join("usr"))?;
    let size_kb = dir_size(&data) / 1024;
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let text = format!(
        "Package: inkra\nVersion: {VERSION}\nArchitecture: {arch}\nMaintainer: Inkra <inkra@users.noreply.github.com>\nInstalled-Size: {size_kb}\nDepends: libc6 (>= 2.31)\nRecommends: xdg-desktop-portal\nSuggests: pandoc\nSection: editors\nPriority: optional\nHomepage: {}\nDescription: Native WYSIWYM Markdown editor\n Inkra renders Markdown as you type: math, diagrams, tables, footnotes,\n export to HTML, PDF and images.\n",
        env!("CARGO_PKG_REPOSITORY")
    );
    std::fs::write(control.join("control"), text).map_err(io)?;
    std::fs::write(control.join("postinst"), "#!/bin/sh\nset -e\ncommand -v update-desktop-database >/dev/null && update-desktop-database -q || true\ncommand -v update-mime-database >/dev/null && update-mime-database /usr/share/mime || true\n").map_err(io)?;
    set_exec(&control.join("postinst"))?;
    let tgz = |dir: &Path, file: &Path| -> Result<Vec<u8>, String> {
        let mut c = Command::new("tar");
        c.arg("--owner=0")
            .arg("--group=0")
            .arg("-C")
            .arg(dir)
            .arg("-czf")
            .arg(file)
            .arg(".");
        run("tar", c)?;
        std::fs::read(file).map_err(io)
    };
    let control_tgz = tgz(&control, &work.join("control.tar.gz"))?;
    let data_tgz = tgz(&data, &work.join("data.tar.gz"))?;
    let bytes = ar(&[
        ("debian-binary", b"2.0\n".to_vec()),
        ("control.tar.gz", control_tgz),
        ("data.tar.gz", data_tgz),
    ]);
    let file = out.join(format!("inkra_{VERSION}_{arch}.deb"));
    std::fs::write(&file, bytes).map_err(io)?;
    Ok(file)
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map_or(0, |rd| {
        rd.flatten()
            .map(|e| {
                let p = e.path();
                if p.is_dir() {
                    dir_size(&p)
                } else {
                    e.metadata().map_or(0, |m| m.len())
                }
            })
            .sum()
    })
}

#[cfg(unix)]
fn set_exec(p: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).map_err(io)
}

#[cfg(not(unix))]
fn set_exec(_: &Path) -> Result<(), String> {
    Ok(())
}

fn rpm(staging: &Path, out: &Path) -> Result<PathBuf, String> {
    let top = staging.join("rpm");
    let tree = staging.join("rpm-tree");
    linux_tree(&root(), &root().join("target/release"), &tree.join("usr"))?;
    for d in ["BUILD", "RPMS", "SOURCES", "SPECS", "SRPMS"] {
        std::fs::create_dir_all(top.join(d)).map_err(io)?;
    }
    let spec = format!(
        "Name: inkra\nVersion: {VERSION}\nRelease: 1\nSummary: Native WYSIWYM Markdown editor\nLicense: MIT OR Apache-2.0\nURL: {}\nAutoReqProv: no\n\n%description\nInkra renders Markdown as you type: math, diagrams, tables, footnotes,\nexport to HTML, PDF and images.\n\n%install\nmkdir -p %{{buildroot}}\ncp -a {}/. %{{buildroot}}/\n\n%files\n/usr/bin/inkra\n/usr/bin/inkra-cli\n/usr/share/applications/io.inkra.Inkra.desktop\n/usr/share/metainfo/io.inkra.Inkra.metainfo.xml\n/usr/share/mime/packages/io.inkra.Inkra.xml\n/usr/share/icons/hicolor/*/apps/*\n/usr/share/doc/inkra\n\n%post\nupdate-desktop-database -q >/dev/null 2>&1 || true\nupdate-mime-database /usr/share/mime >/dev/null 2>&1 || true\n",
        env!("CARGO_PKG_REPOSITORY"),
        tree.display()
    );
    let spec_file = top.join("SPECS/inkra.spec");
    std::fs::write(&spec_file, spec).map_err(io)?;
    let mut c = Command::new("rpmbuild");
    c.arg("-bb")
        .arg("--define")
        .arg(format!("_topdir {}", top.display()))
        .arg(&spec_file);
    run("rpmbuild", c)?;
    let rpms = top.join("RPMS").join(arch());
    let built = std::fs::read_dir(&rpms)
        .map_err(io)?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "rpm"))
        .ok_or("rpmbuild produced nothing")?;
    let file = out.join(built.file_name().unwrap_or_default());
    copy(&built, &file)?;
    Ok(file)
}

fn appimage(staging: &Path, out: &Path) -> Result<PathBuf, String> {
    let dir = staging.join("Inkra.AppDir");
    linux_tree(&root(), &root().join("target/release"), &dir.join("usr"))?;
    copy(
        &root().join("apps/inkra/linux/io.inkra.Inkra.desktop"),
        &dir.join("io.inkra.Inkra.desktop"),
    )?;
    copy(
        &root().join("assets/brand/inkra-256.png"),
        &dir.join("io.inkra.Inkra.png"),
    )?;
    std::fs::write(dir.join("AppRun"), "#!/bin/sh\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\nexec \"$HERE/usr/bin/inkra\" \"$@\"\n").map_err(io)?;
    set_exec(&dir.join("AppRun"))?;
    let file = out.join(format!("Inkra-{VERSION}-{}.AppImage", arch()));
    let mut c = Command::new("appimagetool");
    c.env("ARCH", arch()).arg(&dir).arg(&file);
    run("appimagetool", c)?;
    Ok(file)
}

fn linux(root: &Path, release: &Path, out: &Path) -> Result<Vec<PathBuf>, String> {
    let staging = root.join("target/dist-staging");
    let _ = std::fs::remove_dir_all(&staging);
    let name = format!("inkra-{VERSION}-linux-{}", arch());
    linux_tree(root, release, &staging.join(&name))?;
    copy(
        &root.join("apps/inkra/linux/install.sh"),
        &staging.join(&name).join("install.sh"),
    )?;
    set_exec(&staging.join(&name).join("install.sh"))?;
    let mut files = vec![tar_gz(&staging, &name, out)?, deb(&staging, out)?];
    if have("rpmbuild") {
        files.push(rpm(&staging, out)?);
    } else {
        eprintln!("dist: rpmbuild not found, no .rpm");
    }
    if have("appimagetool") {
        files.push(appimage(&staging, out)?);
    } else {
        eprintln!("dist: appimagetool not found, no AppImage");
    }
    Ok(files)
}

// ---- Windows ----

fn wix_tool(name: &str) -> Option<PathBuf> {
    // `WIX` points at an installation (`bin\`) or the unpacked binaries zip.
    if let Some(w) = std::env::var_os("WIX") {
        for p in [
            PathBuf::from(&w).join("bin").join(format!("{name}.exe")),
            PathBuf::from(&w).join(format!("{name}.exe")),
        ] {
            if p.is_file() {
                return Some(p);
            }
        }
    }
    have(name).then(|| PathBuf::from(name))
}

/// The WiX source: per-user or per-machine install, Start menu shortcut, file
/// associations, optional PATH entry for `inkra-cli`.
fn wxs(release: &Path, root: &Path) -> String {
    let exe = release.join("inkra.exe");
    let cli = release.join("inkra-cli.exe");
    let icon = root.join("assets/brand/inkra.ico");
    let mut assoc = String::new();
    for e in EXTS {
        assoc.push_str(&format!(
            "          <Extension Id=\"{e}\" ContentType=\"text/markdown\"><Verb Id=\"open\" Command=\"Open\" TargetFile=\"InkraExe\" Argument='\"%1\"' /></Extension>\n"
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*" Name="Inkra" Language="1033" Version="{VERSION}" Manufacturer="Inkra" UpgradeCode="6B8C3B5E-6F4E-4C3A-9F7B-1E7C2A9D4B10">
    <Package InstallerVersion="500" Compressed="yes" InstallScope="perMachine" Platform="x64" Description="Inkra Markdown editor" />
    <MajorUpgrade DowngradeErrorMessage="A newer version of Inkra is already installed." />
    <MediaTemplate EmbedCab="yes" />
    <Icon Id="InkraIcon" SourceFile="{icon}" />
    <Property Id="ARPPRODUCTICON" Value="InkraIcon" />
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLDIR" Name="Inkra">
          <Component Id="App" Guid="4E1E7B2A-91C3-4C55-8F0E-0D6B1A2C3E41" Win64="yes">
            <File Id="InkraExe" Source="{exe}" KeyPath="yes" />
            <File Id="InkraCli" Source="{cli}" />
            <ProgId Id="Inkra.Markdown" Description="Markdown Document" Icon="InkraExe" IconIndex="0">
{assoc}            </ProgId>
            <RegistryValue Root="HKLM" Key="Software\Inkra\Capabilities" Name="ApplicationName" Value="Inkra" Type="string" />
            <RegistryValue Root="HKLM" Key="Software\Inkra\Capabilities\FileAssociations" Name=".md" Value="Inkra.Markdown" Type="string" />
            <RegistryValue Root="HKLM" Key="Software\RegisteredApplications" Name="Inkra" Value="Software\Inkra\Capabilities" Type="string" />
            <Environment Id="PathCli" Name="PATH" Value="[INSTALLDIR]" Permanent="no" Part="last" Action="set" System="yes" />
          </Component>
        </Directory>
      </Directory>
      <Directory Id="ProgramMenuFolder">
        <Component Id="Shortcut" Guid="0C5F3E3A-7B21-4C8E-B1A9-2D4E6F8A0B12">
          <Shortcut Id="StartMenu" Name="Inkra" Target="[INSTALLDIR]inkra.exe" WorkingDirectory="INSTALLDIR" Icon="InkraIcon">
            <ShortcutProperty Key="System.AppUserModel.ID" Value="io.inkra.Inkra" />
          </Shortcut>
          <RegistryValue Root="HKCU" Key="Software\Inkra" Name="shortcut" Type="integer" Value="1" KeyPath="yes" />
        </Component>
      </Directory>
    </Directory>
    <Feature Id="Main" Title="Inkra" Level="1">
      <ComponentRef Id="App" />
      <ComponentRef Id="Shortcut" />
    </Feature>
  </Product>
</Wix>
"#,
        icon = icon.display(),
        exe = exe.display(),
        cli = cli.display(),
    )
}

fn windows(root: &Path, release: &Path, out: &Path) -> Result<Vec<PathBuf>, String> {
    let staging = root.join("target/dist-staging");
    let _ = std::fs::remove_dir_all(&staging);
    let name = format!("inkra-{VERSION}-windows-x64");
    let dir = staging.join(&name);
    for f in ["inkra.exe", "inkra-cli.exe"] {
        copy(&release.join(f), &dir.join(f))?;
    }
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "README.md"] {
        copy(&root.join(f), &dir.join(f))?;
    }
    let zip = out.join(format!("{name}.zip"));
    let mut c = Command::new("tar");
    c.arg("-a")
        .arg("-c")
        .arg("-f")
        .arg(&zip)
        .arg("-C")
        .arg(&staging)
        .arg(&name);
    run("zip", c)?;
    let mut files = vec![zip];
    match (wix_tool("candle"), wix_tool("light")) {
        (Some(candle), Some(light)) => {
            let src = staging.join("inkra.wxs");
            std::fs::write(&src, wxs(release, root)).map_err(io)?;
            let obj = staging.join("inkra.wixobj");
            let mut c = Command::new(candle);
            c.arg("-nologo")
                .arg("-arch")
                .arg("x64")
                .arg("-out")
                .arg(&obj)
                .arg(&src);
            run("candle", c)?;
            let msi = out.join(format!("inkra-{VERSION}-x64.msi"));
            let mut l = Command::new(light);
            l.arg("-nologo")
                .arg("-sval")
                .arg("-out")
                .arg(&msi)
                .arg(&obj);
            run("light", l)?;
            files.push(msi);
        }
        _ => eprintln!("dist: WiX Toolset (candle/light) not found, no .msi"),
    }
    Ok(files)
}

// ---- macOS ----

fn macos(root: &Path, release: &Path, out: &Path) -> Result<Vec<PathBuf>, String> {
    let app = out.join("Inkra.app/Contents");
    std::fs::create_dir_all(app.join("MacOS")).map_err(io)?;
    std::fs::create_dir_all(app.join("Resources")).map_err(io)?;
    let plist = std::fs::read_to_string(root.join("apps/inkra/macos/Info.plist"))
        .map_err(io)?
        .replace("$VERSION", VERSION);
    std::fs::write(app.join("Info.plist"), plist).map_err(io)?;
    copy(&release.join("inkra"), &app.join("MacOS/inkra"))?;
    copy(&release.join("inkra-cli"), &app.join("MacOS/inkra-cli"))?;
    copy(
        &root.join("assets/brand/inkra.icns"),
        &app.join("Resources/inkra.icns"),
    )?;
    let dmg = out.join(format!("Inkra-{VERSION}.dmg"));
    let mut c = Command::new("hdiutil");
    c.args([
        "create",
        "-volname",
        "Inkra",
        "-fs",
        "HFS+",
        "-format",
        "UDZO",
        "-srcfolder",
    ])
    .arg(out.join("Inkra.app"))
    .arg(&dmg);
    run("hdiutil", c)?;
    Ok(vec![out.join("Inkra.app"), dmg])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ar_archive_layout() {
        let a = ar(&[("debian-binary", b"2.0\n".to_vec()), ("x", b"abc".to_vec())]);
        assert!(a.starts_with(b"!<arch>\ndebian-binary   "));
        // 8 + (60 + 4) + (60 + 3 + 1 padding)
        assert_eq!(a.len(), 8 + 64 + 64);
        let w = wxs(Path::new("rel"), Path::new("root"));
        assert!(w.contains("<Extension Id=\"md\"") && w.contains(VERSION));
    }
}
