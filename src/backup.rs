use clap::Parser;
use goblin;

#[derive(Parser, Debug)]
#[clap(about, long_about = None)]
struct Args {
    // Reference kernel module to use for obtaining symbol versions
    #[clap(short, long, parse(from_os_str))]
    src: Option<std::path::PathBuf>,

    // Module layout version value to patch into target
    #[clap(short, long, value_parser)]
    module_layout_version: Option<u64>,

    // Keep the original target and write modified output to a new file
    #[clap(short, long, value_parser)]
    keep: Option<bool>,

    // Target kernel module to patch
    #[clap(parse(from_os_str))]
    target: std::path::PathBuf,
}

struct Kmod<'a> {
    elf: Option<&'a goblin::elf::Elf<'a>>,
    backing: Vec<u8>
}

/// Attempts to find a ELF section header matching provided name
///
/// # Arguments
/// * `kmod` - Kernel module to search for section within
/// * `name` - Name of the section header to locate
fn find_section<'a>(kmod: &'a goblin::elf::Elf, name: &str) -> Option<&'a goblin::elf::section_header::SectionHeader> {
    for sh in &kmod.section_headers {
        let sh_name = kmod.shdr_strtab.get_at(sh.sh_name).unwrap_or("");
        if sh_name == name {
            return Some(&sh)
        }
    }
    None
}

fn is_kernel_mod_valid(path: &std::path::PathBuf) -> Result<Kmod, String> {

    let buf = match std::fs::read(path) {
        Ok(buf) => buf,
        Err(e) => { return Err(e.to_string()); }
    };

    let mut kmod = Kmod {
        elf: None,
        backing: buf,
    };
    
    kmod.elf = match goblin::elf::Elf::parse(&kmod.backing) {
        Ok(elf) => Some(elf),
        Err(e) => { return Err(e.to_string()); }
    };

    // Check if target has a "__versions" section. If not, exit.
    // If target kernel was compiled with `CONFIG_MODULE_FORCE_LOAD`, this is
    // OK as target doesn't need patched
    let versions_sh = find_section(&kmod.elf.as_ref().unwrap(), "__versions");
    if versions_sh.is_none() {
        return Err("WARNING: `__versions` section not found in target.\n\
                  This may or may not be a problem depending on if target \
                  kernel was compiled with `CONFIG_MODULE_FORCE_LOAD`. If \
                  this configuration is enabled, the target module to patch \
                  must have a `__versions` section. If disabled, no patching \
                  is required to force load target.".to_string());
    }

    Ok(kmod)
}

fn main() {
    let args = Args::parse();
   
    // Try to open and read target file
    let mut t_buffer = match std::fs::read(args.target) {
        Ok(buf) => buf,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    
    // Try to parse target ELF
    let mut tko = match goblin::elf::Elf::parse(&t_buffer) {
        Ok(binary) => binary,
        Err(e) => {
            eprintln!("Failed to parse target kernel module -- {}", e);
            std::process::exit(1);
        }
    };

    // Check if target has a "__versions" section. If not, exit.
    // If target kernel was compiled with `CONFIG_MODULE_FORCE_LOAD`, this is
    // OK as target doesn't need patched
    let versions_sh = find_section(&tko, "__versions");
    if versions_sh.is_none() {
        println!("WARNING: `__versions` section not found in target.\n\
                  This may or may not be a problem depending on if target \
                  kernel was compiled with `CONFIG_MODULE_FORCE_LOAD`. If \
                  this configuration is enabled, the target module to patch \
                  must have a `__versions` section. If disabled, no patching \
                  is required to force load target.");
        return;
    }

    // Determine if user is providing a reference kernel module with symbol
    // versions or if they wish to only patch the `module_layout` version
    // only
    if args.src.is_none() {
        if args.module_layout_version.is_none() {
            eprintln!("Must specify `-s` or `-m` option to patch target");
            std::process::exit(1);
        }
    }
    else {
        // Check if source kernel module is valid
        //
    }
}
