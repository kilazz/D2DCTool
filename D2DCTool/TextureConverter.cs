using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading.Tasks;

namespace D2DCTool;

public class DdsToNifConverter
{
    public static async Task ConvertAsync(string ddsPath, string nifPath, Action<string>? onProgress)
    {
        await Task.Run(() =>
        {
            using var fs = File.OpenRead(ddsPath);
            using var br = new BinaryReader(fs);

            // 1. Read DDS header (128 bytes)
            if (fs.Length < 128) throw new Exception("File is too small to be a DDS.");

            string magic = Encoding.ASCII.GetString(br.ReadBytes(4));
            if (magic != "DDS ") throw new Exception("Invalid DDS magic signature.");

            br.BaseStream.Position = 12;
            uint height = br.ReadUInt32();
            uint width = br.ReadUInt32();

            br.BaseStream.Position = 28;
            uint mipMapCount = br.ReadUInt32();

            br.BaseStream.Position = 84;
            string fourCC = Encoding.ASCII.GetString(br.ReadBytes(4));

            int blockSize = 0;
            uint pixelFormat = 0; // 4 for DXT1, 6 for DXT5 in NIF format

            if (fourCC == "DXT1") { blockSize = 8; pixelFormat = 4; }
            else if (fourCC == "DXT3") { blockSize = 16; pixelFormat = 5; }
            else if (fourCC == "DXT5") { blockSize = 16; pixelFormat = 6; }
            else throw new Exception($"Unsupported DDS format: {fourCC}. Only DXT1, DXT3, and DXT5 are supported.");

            // If mipmaps are not specified, calculate their count (log2(max(w,h)) + 1)
            if (mipMapCount == 0)
            {
                uint maxDim = Math.Max(width, height);
                mipMapCount = (uint)(Math.Log2(maxDim) + 1);
            }

            onProgress?.Invoke($"DDS Info: {width}x{height}, {fourCC}, {mipMapCount} MipMaps");

            // Calculate mipmap sizes and offsets
            var mipmaps = new List<(uint w, uint h, uint offset)>();
            uint currentOffset = 0;
            uint w = width;
            uint h = height;

            for (int i = 0; i < mipMapCount; i++)
            {
                uint blockWidth = Math.Max(1, (w + 3) / 4);
                uint blockHeight = Math.Max(1, (h + 3) / 4);
                uint size = blockWidth * blockHeight * (uint)blockSize;

                mipmaps.Add((w, h, currentOffset));
                currentOffset += size;

                w = Math.Max(1, w / 2);
                h = Math.Max(1, h / 2);
            }

            uint pixelDataSize = currentOffset;

            // 2. Write NIF file
            using var outFs = File.Create(nifPath);
            using var bw = new BinaryWriter(outFs);

            // NIF Header (Gamebryo 20.3.0.9)
            byte[] headerString = Encoding.ASCII.GetBytes("Gamebryo File Format, Version 20.3.0.9\n");
            bw.Write(headerString);
            bw.Write(0x14030009u); // Version
            bw.Write((byte)1); // Little Endian
            bw.Write(0x20000u); // User Version
            bw.Write(1u); // Num Blocks
            bw.Write((ushort)1); // Num Block Types

            string blockType = "NiPersistentSrcTextureRendererData";
            bw.Write((uint)blockType.Length);
            bw.Write(Encoding.ASCII.GetBytes(blockType));

            bw.Write((ushort)0); // Block Type Index

            // Calculate NiPersistentSrcTextureRendererData block size
            uint blockSizeNif = 87 + (mipMapCount * 12) + pixelDataSize;
            bw.Write(blockSizeNif);

            bw.Write(0u); // Num Strings
            bw.Write(0u); // Max String Length
            bw.Write(0u); // Unknown

            // NiPersistentSrcTextureRendererData data
            bw.Write(pixelFormat);
            bw.Write((byte)0); // Bits per pixel
            bw.Write(-1); // Unknown
            bw.Write(0u); // Unknown
            bw.Write((byte)1); // Flags
            bw.Write(0u); // Unknown
            bw.Write((byte)0); // Unknown

            // Channels
            // Channel 1 (Compressed)
            bw.Write(4u); bw.Write(4u); bw.Write((byte)0); bw.Write((byte)0);
            // Channels 2-4 (Empty)
            for (int i = 0; i < 3; i++)
            {
                bw.Write(19u); bw.Write(5u); bw.Write((byte)0); bw.Write((byte)0);
            }

            bw.Write(-1); // Palette
            bw.Write(mipMapCount);
            bw.Write(0u); // Bytes Per Pixel

            // Write mipmap information
            foreach (var mip in mipmaps)
            {
                bw.Write(mip.w);
                bw.Write(mip.h);
                bw.Write(mip.offset);
            }

            bw.Write(pixelDataSize); // Num Pixels
            bw.Write(pixelDataSize); // Unknown
            bw.Write(1u); // Num Faces
            bw.Write(3u); // Unknown

            // Copy raw pixels from DDS (skipping 128 bytes of DDS header)
            fs.Position = 128;
            byte[] buffer = new byte[8192];
            int read;
            long remaining = pixelDataSize;
            while (remaining > 0 && (read = fs.Read(buffer, 0, (int)Math.Min(buffer.Length, remaining))) > 0)
            {
                outFs.Write(buffer, 0, read);
                remaining -= read;
            }

            // NIF Footer
            bw.Write(1u); // Num Roots
            bw.Write(0u); // Roots[0]

            onProgress?.Invoke($"Successfully converted to {Path.GetFileName(nifPath)}");
        });
    }

    public static async Task<bool> ExtractNifToDdsAsync(string nifPath, string ddsPath, Action<string>? onProgress)
    {
        return await Task.Run(() =>
        {
            byte[] nifData = File.ReadAllBytes(nifPath);

            // Find "NiPersistentSrcTextureRendererData"
            string searchStr = "NiPersistentSrcTextureRendererData";
            byte[] searchBytes = Encoding.ASCII.GetBytes(searchStr);
            int idx = IndexOfBytes(nifData, searchBytes);

            if (idx == -1)
            {
                // Not a texture NIF, silently skip
                return false;
            }

            using var ms = new MemoryStream(nifData);
            using var br = new BinaryReader(ms);

            ms.Position = idx + searchBytes.Length;

            // Skip BlockTypeIndex (2), BlockSize (4), NumStrings (4), MaxStringLen (4), Unknown (4)
            ms.Position += 18;

            uint pixelFormat = br.ReadUInt32();
            string fourCC = "";
            int blockSize = 0;

            // In Gamebryo 20.3.0.9 NiPersistentSrcTextureRendererData:
            // PixelFormat 4 = DXT1
            // PixelFormat 5 = DXT3
            // PixelFormat 6 = DXT5
            if (pixelFormat == 4) { fourCC = "DXT1"; blockSize = 8; }
            else if (pixelFormat == 5) { fourCC = "DXT3"; blockSize = 16; }
            else if (pixelFormat == 6) { fourCC = "DXT5"; blockSize = 16; }
            else throw new Exception($"Unsupported pixel format in NIF: {pixelFormat}");

            // Skip:
            // BitsPerPixel (1)
            // Unknown (4)
            // Unknown (4)
            // Flags (1)
            // Unknown (4)
            // Unknown (1)
            // Channels (4 * 10 = 40)
            // Palette (4)
            ms.Position += 59;

            uint mipMapCount = br.ReadUInt32();
            ms.Position += 4; // Skip BytesPerPixel

            // Read mipmap information
            uint width = 0;
            uint height = 0;
            uint totalRawSize = 0;

            for (int i = 0; i < mipMapCount; i++)
            {
                uint mipW = br.ReadUInt32();
                uint mipH = br.ReadUInt32();
                uint mipOffset = br.ReadUInt32();

                if (i == 0)
                {
                    width = mipW;
                    height = mipH;
                }

                uint bw = Math.Max(1, (mipW + 3) / 4);
                uint bh = Math.Max(1, (mipH + 3) / 4);
                totalRawSize += bw * bh * (uint)blockSize;
            }

            uint numPixels = br.ReadUInt32();
            ms.Position += 12; // Skip Unknown, NumFaces, Unknown

            // Use numPixels as the exact size of the pixel data to read
            uint dataSizeToRead = numPixels;
            if (ms.Position + dataSizeToRead > nifData.Length)
            {
                // If it's too short, just read whatever is left
                dataSizeToRead = (uint)(nifData.Length - ms.Position);
            }

            byte[] rawData = br.ReadBytes((int)dataSizeToRead);

            // Write DDS
            using var outFs = File.Create(ddsPath);
            using var writer = new BinaryWriter(outFs);

            writer.Write(Encoding.ASCII.GetBytes("DDS "));
            writer.Write(124u); // Size

            // Flags: DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT
            uint flags = 0x00000001 | 0x00000002 | 0x00000004 | 0x00001000;
            if (mipMapCount > 1) flags |= 0x00020000; // DDSD_MIPMAPCOUNT
            if (blockSize > 0) flags |= 0x00080000; // DDSD_LINEARSIZE

            writer.Write(flags);
            writer.Write(height);
            writer.Write(width);

            uint pitchOrLinearSize = Math.Max(1, (width + 3) / 4) * Math.Max(1, (height + 3) / 4) * (uint)blockSize;
            writer.Write(pitchOrLinearSize);
            writer.Write(0u); // Depth
            writer.Write(Math.Max(1, mipMapCount)); // MipMapCount (must be at least 1)

            for (int i = 0; i < 11; i++) writer.Write(0u); // Reserved1

            // DDS_PIXELFORMAT
            writer.Write(32u); // Size
            writer.Write(0x00000004u); // Flags (DDPF_FOURCC)
            writer.Write(Encoding.ASCII.GetBytes(fourCC)); // FourCC
            writer.Write(0u); // RGBBitCount
            writer.Write(0u); // RBitMask
            writer.Write(0u); // GBitMask
            writer.Write(0u); // BBitMask
            writer.Write(0u); // ABitMask

            // DDS_HEADER_CAPS
            uint caps1 = 0x00001000; // DDSCAPS_TEXTURE
            if (mipMapCount > 1) caps1 |= 0x00400008; // DDSCAPS_COMPLEX | DDSCAPS_MIPMAP

            writer.Write(caps1); // Caps
            writer.Write(0u); // Caps2
            writer.Write(0u); // Caps3
            writer.Write(0u); // Caps4
            writer.Write(0u); // Reserved2

            // Write raw data
            writer.Write(rawData);

            onProgress?.Invoke($"Extracted {Path.GetFileName(nifPath)} to DDS");
            return true;
        });
    }

    public static async Task BatchConvertDdsToNifAsync(string folderPath, Action<string>? onProgress)
    {
        await Task.Run(async () =>
        {
            var files = Directory.GetFiles(folderPath, "*.dds", SearchOption.AllDirectories);
            int totalFiles = files.Length;
            int successCount = 0;

            if (totalFiles == 0)
            {
                onProgress?.Invoke("No .dds files found in the selected directory.");
                return;
            }

            for (int i = 0; i < totalFiles; i++)
            {
                var file = files[i];
                try
                {
                    onProgress?.Invoke($"[{i + 1}/{totalFiles}] Converting {Path.GetFileName(file)}...");
                    string nifPath = Path.ChangeExtension(file, ".nif");
                    await ConvertAsync(file, nifPath, null);
                    successCount++;
                }
                catch (Exception ex)
                {
                    onProgress?.Invoke($"[{i + 1}/{totalFiles}] Error converting {Path.GetFileName(file)}: {ex.Message}");
                }
            }
            onProgress?.Invoke($"Batch conversion complete! Successfully converted {successCount}/{totalFiles} DDS files to NIF.");
        });
    }

    public static async Task BatchExtractNifToDdsAsync(string folderPath, Action<string>? onProgress)
    {
        await Task.Run(async () =>
        {
            var files = Directory.GetFiles(folderPath, "*.nif", SearchOption.AllDirectories);
            int totalFiles = files.Length;
            int successCount = 0;
            int skippedCount = 0;

            if (totalFiles == 0)
            {
                onProgress?.Invoke("No .nif files found in the selected directory.");
                return;
            }

            for (int i = 0; i < totalFiles; i++)
            {
                var file = files[i];
                try
                {
                    onProgress?.Invoke($"[{i + 1}/{totalFiles}] Extracting {Path.GetFileName(file)}...");
                    string ddsPath = Path.ChangeExtension(file, ".dds");
                    bool isTexture = await ExtractNifToDdsAsync(file, ddsPath, null);
                    if (isTexture)
                    {
                        successCount++;
                    }
                    else
                    {
                        skippedCount++;
                    }
                }
                catch (Exception ex)
                {
                    onProgress?.Invoke($"[{i + 1}/{totalFiles}] Error extracting {Path.GetFileName(file)}: {ex.Message}");
                }
            }
            onProgress?.Invoke($"Batch extraction complete! Successfully extracted {successCount} textures. Skipped {skippedCount} non-texture NIFs. Total processed: {totalFiles}.");
        });
    }

    private static int IndexOfBytes(byte[] array, byte[] pattern)
    {
        for (int i = 0; i <= array.Length - pattern.Length; i++)
        {
            bool match = true;
            for (int j = 0; j < pattern.Length; j++)
            {
                if (array[i + j] != pattern[j])
                {
                    match = false;
                    break;
                }
            }
            if (match) return i;
        }
        return -1;
    }
}
