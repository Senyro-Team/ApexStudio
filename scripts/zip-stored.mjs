export function zipStored(files) {
  const local = [],
    central = [];
  let offset = 0;
  const crc32 = (bytes) => {
    let c = 0xffffffff;
    for (const b of bytes) {
      c ^= b;
      for (let i = 0; i < 8; i++) c = (c >>> 1) ^ (c & 1 ? 0xedb88320 : 0);
    }
    return (c ^ 0xffffffff) >>> 0;
  };
  for (const [name, data] of files) {
    const filename = Buffer.from(name),
      bytes = Buffer.from(data),
      crc = crc32(bytes),
      h = Buffer.alloc(30 + filename.length);
    h.writeUInt32LE(0x04034b50);
    h.writeUInt16LE(20, 4);
    h.writeUInt16LE(0x800, 6);
    h.writeUInt32LE(crc, 14);
    h.writeUInt32LE(bytes.length, 18);
    h.writeUInt32LE(bytes.length, 22);
    h.writeUInt16LE(filename.length, 26);
    filename.copy(h, 30);
    local.push(h, bytes);
    const c = Buffer.alloc(46 + filename.length);
    c.writeUInt32LE(0x02014b50);
    c.writeUInt16LE(20, 4);
    c.writeUInt16LE(20, 6);
    c.writeUInt16LE(0x800, 8);
    c.writeUInt32LE(crc, 16);
    c.writeUInt32LE(bytes.length, 20);
    c.writeUInt32LE(bytes.length, 24);
    c.writeUInt16LE(filename.length, 28);
    c.writeUInt32LE(offset, 42);
    filename.copy(c, 46);
    central.push(c);
    offset += h.length + bytes.length;
  }
  const size = central.reduce((sum, b) => sum + b.length, 0),
    end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(size, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, ...central, end]);
}
