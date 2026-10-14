import { createHash } from 'node:crypto';

export function chromeArchive({ executable = '#!/bin/sh\nexit 0\n', resource = 'full distribution resource', unsafeLink = false } = {}) {
  const files = [
    { name: 'chrome-linux64/chrome', content: Buffer.from(executable), mode: 0o100755 },
    { name: 'chrome-linux64/resources.pak', content: Buffer.from(resource), mode: 0o100644 },
    { name: 'chrome-linux64/locales/en-US.pak', content: Buffer.from('locale resource'), mode: 0o100644 },
  ];
  if (unsafeLink) files.push({ name: 'chrome-linux64/escape', content: Buffer.from('../../outside'), mode: 0o120777 });
  const chunks = [];
  const central = [];
  let offset = 0;
  for (const file of files) {
    const name = Buffer.from(file.name);
    let crc = 0xffffffff;
    for (const byte of file.content) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    crc = (crc ^ 0xffffffff) >>> 0;
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(file.content.length, 18);
    header.writeUInt32LE(file.content.length, 22);
    header.writeUInt16LE(name.length, 26);
    chunks.push(header, name, file.content);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(0x0314, 4);
    entry.writeUInt16LE(20, 6);
    entry.writeUInt32LE(crc, 16);
    entry.writeUInt32LE(file.content.length, 20);
    entry.writeUInt32LE(file.content.length, 24);
    entry.writeUInt16LE(name.length, 28);
    entry.writeUInt32LE((file.mode << 16) >>> 0, 38);
    entry.writeUInt32LE(offset, 42);
    central.push(entry, name);
    offset += header.length + name.length + file.content.length;
  }
  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  const archive = Buffer.concat([...chunks, directory, end]);
  return { archive, sha256: createHash('sha256').update(archive).digest('hex') };
}
