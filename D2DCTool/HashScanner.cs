using System;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;

namespace D2DCTool;

public static class HashScanner
{
    public static async Task ScanAndVerifyAsync(string xmlDir, Action<string> log)
    {
        await Task.Run(() =>
        {
            string dictFile = "hash_dictionary.txt";
            string outFile = "unknown_hashes.txt";

            // 1. Load dictionary
            var knownStrings = new HashSet<string>(); //OrdinalIgnoreCase
            if (File.Exists(dictFile))
            {
                foreach (var line in File.ReadAllLines(dictFile))
                {
                    string s = line.Trim();
                    if (!string.IsNullOrEmpty(s))
                    {
                        knownStrings.Add(s);
                    }
                }
            }

            log($"Loaded {knownStrings.Count} known strings from dictionary.");

            var knownHashes = new HashSet<uint>();
            foreach (var s in knownStrings)
            {
                knownHashes.Add(XmlConverter.GetHashValue(s));
            }

            // 2. Scan files
            var allFoundHashes = new HashSet<uint>();
            int binaryCount = 0;
            int textCount = 0;

            string[] files = Directory.GetFiles(xmlDir, "*.xml", SearchOption.AllDirectories);
            log($"Scanning {files.Length} XML files...");

            Regex textRegex = new(@"\b(?:Name|Data)=""([0-9A-Fa-f]{8})""", RegexOptions.Compiled);

            for (int i = 0; i < files.Length; i++)
            {
                string file = files[i]; if (i > 0 && i % 500 == 0)
                {
                    log($"Scanning... {i}/{files.Length}");
                }

                var hashes = ParseBinaryXml(file);
                if (hashes != null)
                {
                    foreach (var h in hashes)
                    {
                        allFoundHashes.Add(h);
                    }

                    binaryCount++;
                }
                else
                {
                    hashes = ParseTextXml(file, textRegex);
                    foreach (var h in hashes)
                    {
                        allFoundHashes.Add(h);
                    }

                    textCount++;
                }
            }

            log($"Processed binary XMLs: {binaryCount}, text XMLs: {textCount}");
            log($"Total unique hashes found: {allFoundHashes.Count}");

            var unknownHashes = new HashSet<uint>(allFoundHashes);
            unknownHashes.ExceptWith(knownHashes);

            int knownFound = allFoundHashes.Count - unknownHashes.Count;
            log($"Already KNOWN (in dictionary): {knownFound}");
            log($"Remaining UNKNOWN: {unknownHashes.Count}");

            // Rewrite dictionary
            var sortedStrings = new List<string>(knownStrings);
            sortedStrings.Sort(); //StringComparer.OrdinalIgnoreCase
            File.WriteAllLines(dictFile, sortedStrings);

            // Write unknown hashes
            var sortedUnknowns = new List<uint>(unknownHashes);
            sortedUnknowns.Sort();
            using (var sw = new StreamWriter(outFile))
            {
                foreach (var h in sortedUnknowns)
                {
                    sw.WriteLine($"{h:X8}");
                }
            }
            log($"[OK] Cleaned dictionary and unknown hashes saved.");
        });
    }

    private static HashSet<uint>? ParseBinaryXml(string filepath)
    {
        try
        {
            using var fs = File.OpenRead(filepath);
            using var br = new BinaryReader(fs);

            string headerStr = "";
            while (fs.Position < fs.Length)
            {
                char c = br.ReadChar(); headerStr += c; if (c == '\n')
                {
                    break;
                }
            }
            if (!headerStr.StartsWith("Gamebryo File Format"))
            {
                return null;
            }

            uint version = br.ReadUInt32(); byte endian = br.ReadByte(); bool isLittleEndian = endian != 0; uint ReadU32() { uint val = br.ReadUInt32(); return isLittleEndian ? val : BinaryPrimitives.ReverseEndianness(val); }
            ushort ReadU16() { ushort val = br.ReadUInt16(); return isLittleEndian ? val : BinaryPrimitives.ReverseEndianness(val); }
            uint userVersion = ReadU32(); uint numBlocks = ReadU32(); ushort numBlockTypes = ReadU16(); var blockTypes = new List<string>(); for (int i = 0; i
                                    < numBlockTypes; i++) { uint len = ReadU32(); blockTypes.Add(Encoding.UTF8.GetString(br.ReadBytes((int)len))); }
            var blockTypeIndices = new ushort[numBlocks]; for (int i = 0; i
                                    < numBlocks; i++)
            {
                blockTypeIndices[i] = ReadU16();
            }

            var blockSizes = new uint[numBlocks]; for (int i = 0; i
                                    < numBlocks; i++)
            {
                blockSizes[i] = ReadU32();
            }

            uint numStrings = ReadU32(); uint maxStringLen = ReadU32(); for (int i = 0; i
                                    < numStrings; i++) { uint len = ReadU32(); fs.Seek(len, SeekOrigin.Current); }
            uint numGroups = ReadU32(); for (int i = 0; i
                                    < numGroups; i++)
            {
                fs.Seek(4, SeekOrigin.Current);
            }

            if (numBlocks != 1 || !blockTypes[blockTypeIndices[0]].Contains("xml::dom::CStreamableNode"))
            {
                return new HashSet<uint>();
            }

            uint nodeCount = ReadU32(); uint totalAttrCount = ReadU32(); uint stringBlockSize = ReadU32(); fs.Seek(stringBlockSize, SeekOrigin.Current); var hashes = new HashSet<uint>(); void ReadNode()
            {
                byte flags = br.ReadByte(); uint data = ReadU32(); hashes.Add(data); if ((flags & 0x02) != 0)
                {
                    uint attrCount = ((flags & 0x08) == 0) ? ReadU32() : br.ReadByte(); for (int i = 0; i
                                    < attrCount; i++) { uint attrData = ReadU32(); hashes.Add(attrData); }
                }
                if ((flags & 0x01) != 0)
                {
                    uint childCount = ((flags & 0x08) == 0) ? ReadU32() : br.ReadByte(); for (int i = 0; i
                                    < childCount; i++) { ReadNode(); }
                }
            }
            ReadNode(); return hashes;
        }
        catch { return null; }
    }
    private static HashSet<uint> ParseTextXml(string filepath, Regex regex)
    {
        var hashes = new HashSet<uint>(); try
        {
            string content = File.ReadAllText(filepath); var matches = regex.Matches(content); foreach (Match match in matches)
            {
                if (match.Groups.Count > 1)
                {
                    hashes.Add(Convert.ToUInt32(match.Groups[1].Value, 16));
                }
            }
        }
        catch { }
        return hashes;
    }
}
