using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Text;
using System.Threading.Tasks;

namespace D2DCTool;

public class Dv2Entry
{
    public string Name { get; set; } = string.Empty;
    public uint StartOffset { get; set; }
    public uint CompressedSize { get; set; }
    public uint UncompressedSize { get; set; }
}

public class Dv2Archive
{
    private const int BoundarySize = 32768; // 32 KB boundary

    public static async Task<List<Dv2Entry>> ReadEntriesAsync(string dv2Path)
    {
        return await Task.Run(() =>
        {
            using var fs = File.OpenRead(dv2Path);
            using var br = new BinaryReader(fs);

            uint version = br.ReadUInt32();
            if (version != 4 && version != 5)
            {
                throw new Exception($"Unsupported DV2 version: {version}");
            }

            if (version == 5)
            {
                br.ReadUInt32(); // unknown1
                br.ReadUInt32(); // unknown2
            }

            byte isAligned = br.ReadByte();
            byte isPacked = br.ReadByte();
            uint dataStartOffset = br.ReadUInt32();
            uint filenamesSize = br.ReadUInt32();

            // Read filenames
            byte[] nameBytes = br.ReadBytes((int)filenamesSize);
            var names = new List<string>();
            int currentStart = 0;
            for (int i = 0; i < nameBytes.Length; i++)
            {
                if (nameBytes[i] == 0)
                {
                    if (i > currentStart)
                    {
                        names.Add(Encoding.UTF8.GetString(nameBytes, currentStart, i - currentStart));
                    }

                    currentStart = i + 1;
                }
            }

            uint fileCount = br.ReadUInt32();
            var entries = new List<Dv2Entry>();

            for (int i = 0; i < fileCount; i++) { entries.Add(new Dv2Entry { Name = names[i], StartOffset = br.ReadUInt32(), CompressedSize = br.ReadUInt32(), UncompressedSize = br.ReadUInt32() }); }
            return entries;
        });
    }
    public static async Task UnpackAsync(string dv2Path, string outDir, Action<string> onProgress)
    {
        await Task.Run(() =>
        {
            using var fs = File.OpenRead(dv2Path);
            using var br = new BinaryReader(fs);

            uint version = br.ReadUInt32();
            if (version != 4 && version != 5)
            {
                throw new Exception($"Unsupported DV2 version: {version}");
            }

            if (version == 5)
            {
                br.ReadUInt32(); // unknown1
                br.ReadUInt32(); // unknown2
            }

            byte isAligned = br.ReadByte();
            byte isPacked = br.ReadByte();
            uint dataStartOffset = br.ReadUInt32();
            uint filenamesSize = br.ReadUInt32();

            // Read filenames
            byte[] nameBytes = br.ReadBytes((int)filenamesSize);
            var names = new List<string>();
            int currentStart = 0;
            for (int i = 0; i < nameBytes.Length; i++)
            {
                if (nameBytes[i] == 0)
                {
                    if (i > currentStart)
                    {
                        names.Add(Encoding.UTF8.GetString(nameBytes, currentStart, i - currentStart));
                    }

                    currentStart = i + 1;
                }
            }

            uint fileCount = br.ReadUInt32();
            var entries = new List<Dv2Entry>();

            for (int i = 0; i < fileCount; i++) { entries.Add(new Dv2Entry { Name = names[i], StartOffset = br.ReadUInt32(), CompressedSize = br.ReadUInt32(), UncompressedSize = br.ReadUInt32() }); }
            Directory.CreateDirectory(outDir); for (int i = 0; i
                                < entries.Count; i++)
            {
                var entry = entries[i]; onProgress?.Invoke($"Extracting ({i + 1} {entries.Count}): {entry.Name}"); string outPath = Path.Combine(outDir, entry.Name.Replace('\\', Path.DirectorySeparatorChar)); Directory.CreateDirectory(Path.GetDirectoryName(outPath)!); fs.Position = dataStartOffset + entry.StartOffset; using var outStream = File.Create(outPath); if (entry.UncompressedSize > 0) // Compressed with Zlib
                {
                    using var zlib = new ZLibStream(fs, CompressionMode.Decompress, leaveOpen: true);
                    byte[] buffer = new byte[8192];
                    int read;
                    long totalRead = 0;
                    while (totalRead < entry.UncompressedSize && (read = zlib.Read(buffer, 0, (int)Math.Min(buffer.Length, entry.UncompressedSize - totalRead))) > 0)
                    {
                        outStream.Write(buffer, 0, read);
                        totalRead += read;
                    }
                }
                else // Uncompressed
                {
                    byte[] buffer = new byte[8192];
                    long remaining = entry.CompressedSize;
                    while (remaining > 0)
                    {
                        int read = fs.Read(buffer, 0, (int)Math.Min(buffer.Length, remaining));
                        outStream.Write(buffer, 0, read);
                        remaining -= read;
                    }
                }
            }
            onProgress?.Invoke("Unpack complete!");
        });
    }

    public static async Task PackAsync(string sourceDir, string outDv2Path, bool compress, CompressionLevel compLevel, Action<string> onProgress)
    {
        await Task.Run(() =>
        {
            var files = Directory.GetFiles(sourceDir, "*.*", SearchOption.AllDirectories);
            using var fs = File.Create(outDv2Path);
            using var bw = new BinaryWriter(fs);

            using var namesMs = new MemoryStream();
            var entries = new List<Dv2Entry>();

            foreach (var file in files)
            {
                string relativePath = Path.GetRelativePath(sourceDir, file).Replace(Path.DirectorySeparatorChar, '\\');
                byte[] nameBytes = Encoding.UTF8.GetBytes(relativePath);
                namesMs.Write(nameBytes);
                namesMs.WriteByte(0); // Null terminator
                entries.Add(new Dv2Entry { Name = relativePath });
            }

            byte[] namesBlock = namesMs.ToArray();

            uint headerSize = 22; // V5 header size
            uint fileCount = (uint)entries.Count;
            uint dirBlockSize = fileCount * 12;

            uint totalHeaderArea = headerSize + (uint)namesBlock.Length + 4 + dirBlockSize;
            uint dataStartOffset = (uint)GetNextBoundary(totalHeaderArea);

            // Write V5 Header
            bw.Write((uint)5); // ID
            bw.Write((uint)1); // Unknown 1
            bw.Write((uint)4); // Unknown 2
            bw.Write((byte)0); // IsAligned
            bw.Write((byte)1); // IsPacked
            bw.Write(dataStartOffset);
            bw.Write((uint)namesBlock.Length);

            bw.Write(namesBlock);
            bw.Write(fileCount);

            long dirPosition = fs.Position;
            bw.Write(new byte[dirBlockSize]);

            PadTo(fs, dataStartOffset);

            uint currentOffset = 0;

            for (int i = 0; i < files.Length; i++) { onProgress?.Invoke($"Packing ({i + 1} {files.Length}): {entries[i].Name}"); entries[i].StartOffset = currentOffset; using var inStream = File.OpenRead(files[i]); long fileLength = inStream.Length; if (compress) { entries[i].UncompressedSize = (uint)fileLength; long startPos = fs.Position; using (var zlib = new ZLibStream(fs, compLevel, leaveOpen: true)) { inStream.CopyTo(zlib); } entries[i].CompressedSize = (uint)(fs.Position - startPos); } else { entries[i].UncompressedSize = 0; entries[i].CompressedSize = (uint)fileLength; inStream.CopyTo(fs); } currentOffset += entries[i].CompressedSize; if (!compress) { long paddedPos = GetNextBoundary(fs.Position); currentOffset += (uint)(paddedPos - fs.Position); PadTo(fs, paddedPos); } }
            fs.Position = dirPosition; foreach (var entry in entries) { bw.Write(entry.StartOffset); bw.Write(entry.CompressedSize); bw.Write(entry.UncompressedSize); }
            onProgress?.Invoke("Pack complete!");
        });
    }
    private static long GetNextBoundary(long pos)
    {
        return (pos + BoundarySize - 1) / BoundarySize * BoundarySize;
    }

    private static void PadTo(Stream stream, long targetPosition)
    {
        long diff = targetPosition - stream.Position;
        if (diff > 0)
        {
            stream.Write(new byte[diff]);
        }
    }
}
