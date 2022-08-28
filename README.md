# candycorn
`candycorn` is a tool to patch a compiled kernel module to force insertion into systems where the exact kernel source tree is unavailable or hard to find. The target kernel module can first be compiled with different but similar kernel source then patched with `candycorn` to force insertion into target kernel.

`candycorn` will not handle patching for kernel modules that must be signed to be loaded (`CONFIG_MODULE_SIG`). `candycorn` does not guarantee that the patched kernel module is safe to load.

## Example
Patch just `module_layout` CRC with provided value 0xDEADBEEF:

`candycorn -m 3735928559 ./target.ko`

Patch all symbol CRCs in find in target that are also in the source kernel module compiled for target kernel (`reference.ko`)
`candycorn -s ./reference.ko ./target.ko`

## How it works
Linux kernel modules are typically compiled with a kernel source tree. There are a number of configuration options that affect how kernel modules are verified upon being loaded into a system:
* `CONFIG_MODVERSIONS` - When enabled all kernel symbols have a CRC computed. A copy of the CRC is stored in the kernel and each kernel module. When the kernel module is loaded at runtime, verification checks are performed on the kernel module symbol CRCs to ensure they match the CRC of the built kernel.
* `CONFIG_MODULE_FORCE_LOAD` - When enabled allows bypassing the CRC version checking when a kernel module is loaded
* `CONFIG_MODULE_SIG` - When enabled performs signature checking of kernel modules to ensure they have not been modified
* `CONFIG_MODULE_SIG_FORCE` - When enabled allows loading of unsigned kernel modules when `CONFIG_MODULE_SIG` is enabled

When `CONFIG_MODVERSIONS` is enabled, kernel modules will be compiled with a ELF section called `__versions` which contains an array of `modversion_info` structures. Each entry is a CRC value followed by the symbol name. Every kernel module has at least 1 symbol called `module_layout` which has its version checked when version checking is enabled. The CRC value for `module_layout` can be obtained from the compiled kernel image or by dumping this CRC from another kernel module that was compiled for the target kernel. In the event the kernel module has additional symbol imports, each of those CRCs will also be verified at runtime.

In cases where we don't care about symbol incompatibility, we can patch the CRC statically such that the kernel module is treated as if loaded with `--force` (even when the target kernel has `CONFIG_MODULE_FORCE_LOAD` disabled).


