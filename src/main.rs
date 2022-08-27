use clap::Parser;
use goblin;
use std::collections::HashMap;

#[derive(Parser, Debug)]
#[clap(about, long_about = None)]
struct Args {
    /// Reference kernel module to use for obtaining symbol versions
    #[clap(short, long, parse(from_os_str))]
    src: Option<std::path::PathBuf>,

    /// Module layout version value to patch into target
    #[clap(short, long, value_parser, required_unless("src"))]
    module_layout_version: Option<u64>,

    /// Keep the original target and write modified output to a new file
    #[clap(short, long, value_parser)]
    keep: Option<bool>,

    /// Target kernel module to patch
    #[clap(required(true), parse(from_os_str))]
    target: std::path::PathBuf,
}

/// Attempts to find a ELF section header matching provided name
///
/// # Arguments
/// * `kmod` - Kernel module to search for section within
/// * `name` - Name of the section header to locate
fn find_section<'a>(kmod: &'a goblin::elf::Elf, name: &str) 
    -> Option<&'a goblin::elf::section_header::SectionHeader> {
   
    for sh in &kmod.section_headers {
        let sh_name = kmod.shdr_strtab.get_at(sh.sh_name).unwrap_or("");
        if sh_name == name {
            return Some(&sh)
        }
    }
    None
}

/// Finds first null byte in a byte slice and creates `String` from beginning of
/// slice up to null byte. If no null byte is found in the slice, the `String`
/// will be the entire byte slice
fn str_from_u8(utf8: &[u8]) -> String {
    // Find null byte
    let mut null_idx = utf8.len();
    for i in 0 .. utf8.len() {
        if utf8[i] == 0 {
            null_idx = i;
            break;
        }
    }

    std::string::String::from_utf8_lossy(&utf8[0 .. null_idx]).into_owned()
}

#[derive(Debug)]
struct SymVersion {
    crc: u64,
    offset: usize
}

/// Produces a hash map of symbol versioning info given a kernel module's ELF 
/// metadata and backing byte content
fn get_versions(info: &goblin::elf::Elf, mod_data: &Vec<u8>) 
    -> Option<HashMap<String, SymVersion>> {
   
     // Find location of `__versions` section
    const MOD_VER_INFO_NAME_OFFSET: usize = 8;
    const MOD_VER_INFO_SIZE: usize = 64;

    // Make sure `__versions` section is present
    let vers_sh = find_section(info, "__versions")?;

    let mut start_idx: usize = vers_sh.sh_offset as usize;

    // Check if end_idx is sane value (multiple of ModVersionInfo size)
    let entries: usize = vers_sh.sh_size as usize / MOD_VER_INFO_SIZE;
    if (entries * MOD_VER_INFO_SIZE) != vers_sh.sh_size as usize {
        eprintln!("ERROR: `__versions` section unexpected size");
        return None;
    }

    let mut versions = HashMap::new();

    // Parse all version entries and populate map with copies of data
    // Borrow checker will prevent modifying backing data later if we use
    // references
    for _ in 0 .. entries {
        let end_idx: usize = start_idx + MOD_VER_INFO_SIZE;
        let ver_info_name = &mod_data[(start_idx + MOD_VER_INFO_NAME_OFFSET) 
                                        .. end_idx];

        let sym_ver = SymVersion {
            crc: u64::from_le_bytes(
                     (&mod_data[start_idx .. 
                      (start_idx + MOD_VER_INFO_NAME_OFFSET)])
                     .try_into().unwrap()),
            offset: start_idx,
        };
        versions.insert(str_from_u8(&ver_info_name), sym_ver); 
        
        start_idx += MOD_VER_INFO_SIZE;
    }
    Some(versions)
}

fn main() {
    let args = Args::parse();
   
    // Try to open and read target file
    let mut out_path = args.target.clone();
    let mut t_buffer = match std::fs::read(args.target) {
        Ok(buf) => buf,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    
    // Try to parse target ELF
    let t_ko = match goblin::elf::Elf::parse(&t_buffer) {
        Ok(binary) => binary,
        Err(e) => {
            eprintln!("Failed to parse target kernel module -- {}", e);
            std::process::exit(1);
        }
    };

    // Check if target has a "__versions" section. If not, exit.
    // If target kernel was compiled with `CONFIG_MODULE_FORCE_LOAD`, this is
    // OK as target doesn't need patched
    let versions_sh = find_section(&t_ko, "__versions");
    if versions_sh.is_none() {
        println!("WARNING: `__versions` section not found in target.\n\
                  This may or may not be a problem depending on if target \
                  kernel was compiled with `CONFIG_MODULE_FORCE_LOAD`. If \
                  this configuration is enabled, the target module to patch \
                  must have a `__versions` section. If disabled, no patching \
                  is required to force load target.");
        return;
    }

    // TODO: Improve unwrap by returning useful errors
    let t_versions = get_versions(&t_ko, &t_buffer).unwrap();

    // Get endianness 
    // We no longer need the target ELF data and holding it any longer will
    // prevent updating the backing target buffer
    drop(t_ko);

    // See if source kernel module was provided and handle
    if args.src.is_some() {
        let s_buffer = match std::fs::read(args.src.as_ref().unwrap()) {
            Ok(buf) => buf,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        };

        let s_ko = match goblin::elf::Elf::parse(&s_buffer) {
            Ok(bin) => bin,
            Err(e) => {
                eprintln!("Failed to parse source kernel module -- {}", e);
                std::process::exit(1);
            }
        };
        let s_versions = match get_versions(&s_ko, &s_buffer) {
            Some(s) => s,
            None => { std::process::exit(1); },
        };

        for name in t_versions.keys() {
            match s_versions.get(name) {
                Some(s_ver) => { 
                    let off = t_versions[name].offset;
                    println!(
                        "Found version symbol \"{}\" in source with CRC 0x{:x}",
                        name, s_ver.crc);
                    t_buffer.splice(off..off+8, s_ver.crc.to_le_bytes());
                },
                None => {},
            }
        }
    }

    // If user provided "layout_module" crc manually, apply it now. This will
    // overwrite the "layout_module" provided by the source kernel module if
    // it existed
    if args.module_layout_version.is_some() {
        let t_module_layout = t_versions.get("module_layout")
                    .expect("Unable to find \"module_layout\" symbol version");
        let off = t_module_layout.offset;
        println!("Patching \"{}\" in target with CRC 0x{:x}", "module_layout", 
                    args.module_layout_version.unwrap());
        t_buffer.splice(off..off+8, args.module_layout_version.unwrap()
                        .to_le_bytes());
    }

    // Write out result
    // TODO: Handle keep option or provide new option to specify output path
    let mut new_filename = out_path.file_name().unwrap().to_os_string();
    new_filename.push(".patch");
    out_path.set_file_name(new_filename);
    std::fs::write(std::path::Path::new("./test.ko"), t_buffer).unwrap();
    println!("Done!");
}
