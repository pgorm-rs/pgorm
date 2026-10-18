// Whether a native addon is stripped, read from its bytes so that any runner
// can check any platform's binary. Loading needs only the symbols the dynamic
// linker resolves: an ELF library's dynamic symbol table, and a Mach-O
// library's external and undefined symbols, which between them hold
// `napi_register_module_v1`, the one symbol a Node-API host looks up.
// Stripping removes the rest, ELF's `.symtab` and Mach-O's local symbols, so
// a stripped addon has none of those left. The one entry macOS's `strip`
// writes itself, a debugger stab, is not counted.

const ELF_MAGIC = 0x7f454c46;
const MH_MAGIC_64 = 0xfeedfacf;
const SHT_SYMTAB = 2;
const LC_SYMTAB = 0x2;
const LC_DYSYMTAB = 0xb;
const N_STAB = 0xe0;

/**
 * How many symbols `data` carries beyond the ones its loading needs, or
 * undefined when it is neither a 64-bit little-endian ELF nor a 64-bit
 * Mach-O file, the formats of the platforms a release builds.
 */
export function localSymbols(data: Uint8Array): number | undefined {
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  if (view.getUint32(0, false) === ELF_MAGIC) {
    // ELFCLASS64 and ELFDATA2LSB.
    if (data[4] !== 2 || data[5] !== 1) return undefined;
    const sections = Number(view.getBigUint64(0x28, true));
    const size = view.getUint16(0x3a, true);
    let count = 0;
    for (let index = 0; index < view.getUint16(0x3c, true); index++) {
      const header = sections + index * size;
      if (view.getUint32(header + 4, true) !== SHT_SYMTAB) continue;
      const bytes = Number(view.getBigUint64(header + 0x20, true));
      const entry = Number(view.getBigUint64(header + 0x38, true));
      count += entry === 0 ? 0 : bytes / entry;
    }
    return count;
  }
  if (view.getUint32(0, true) === MH_MAGIC_64) {
    let table = 0;
    let first = 0;
    let locals = 0;
    let offset = 32;
    for (let index = 0; index < view.getUint32(16, true); index++) {
      const command = view.getUint32(offset, true);
      if (command === LC_SYMTAB) table = view.getUint32(offset + 8, true);
      if (command === LC_DYSYMTAB) {
        first = view.getUint32(offset + 8, true);
        locals = view.getUint32(offset + 12, true);
      }
      offset += view.getUint32(offset + 4, true);
    }
    let count = 0;
    // Each nlist_64 entry is 16 bytes, its n_type the byte at offset 4.
    for (let index = first; index < first + locals; index++) {
      if ((view.getUint8(table + index * 16 + 4) & N_STAB) === 0) count++;
    }
    return count;
  }
  return undefined;
}
