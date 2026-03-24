using System;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using System.Xml.Linq;

namespace D2DCTool
{
    public class GamebryoNode
    {
        public uint Data;
        public string? Value;
        public List<GamebryoAttribute> Attributes = [];
        public List<GamebryoNode> Children = [];
    }

    public class GamebryoAttribute
    {
        public uint Data;
        public string Value = "";
    }

    public static class XmlConverter
    {
        private static readonly Dictionary<uint, string> _hashToString = new();

        public static void LoadHashes(IEnumerable<string> strings)
        {
            foreach (var s in strings)
            {
                uint hash = GetHashValue(s);
                _hashToString[hash] = s;
            }
        }

        public static uint GetHashValue(string text)
        {
            if (string.IsNullOrEmpty(text))
            {
                return 0;
            }

            uint hash = 0;
            foreach (char c in text)
            {
                hash = (hash * 0x21 + (byte)c) % 0xFFFFFFFF;
            }
            return hash;
        }

        private static uint ReadUInt32E(BinaryReader br, bool isLittleEndian)
        {
            uint val = br.ReadUInt32();
            return isLittleEndian ? val : BinaryPrimitives.ReverseEndianness(val);
        }

        private static ushort ReadUInt16E(BinaryReader br, bool isLittleEndian)
        {
            ushort val = br.ReadUInt16();
            return isLittleEndian ? val : BinaryPrimitives.ReverseEndianness(val);
        }

        private static void WriteUInt32E(BinaryWriter bw, uint val, bool isLittleEndian)
        {
            bw.Write(isLittleEndian ? val : BinaryPrimitives.ReverseEndianness(val));
        }

        private static void WriteUInt16E(BinaryWriter bw, ushort val, bool isLittleEndian)
        {
            bw.Write(isLittleEndian ? val : BinaryPrimitives.ReverseEndianness(val));
        }

        public static async Task<bool> ExtractBinaryXmlToRealXmlAsync(string nifPath, string xmlPath, Action<string> log)
        {
            try
            {
                byte[] data = await File.ReadAllBytesAsync(nifPath);
                using var ms = new MemoryStream(data);
                using var br = new BinaryReader(ms);

                // Read header string
                string headerStr = "";
                while (ms.Position < ms.Length)
                {
                    char c = br.ReadChar();
                    headerStr += c;
                    if (c == '\n')
                    {
                        break;
                    }
                }

                if (!headerStr.StartsWith("Gamebryo File Format"))
                {
                    log("Not a valid Gamebryo file.");
                    return false;
                }

                uint version = br.ReadUInt32(); // Version is always Little Endian
                byte endian = br.ReadByte();
                bool isLittleEndian = endian != 0;

                if (!isLittleEndian)
                {
                    log("Detected Big Endian (Xbox 360) format. Swapping bytes...");
                }

                uint userVersion = ReadUInt32E(br, isLittleEndian);
                uint numBlocks = ReadUInt32E(br, isLittleEndian);
                ushort numBlockTypes = ReadUInt16E(br, isLittleEndian);

                string[] blockTypes = new string[numBlockTypes];
                for (int i = 0; i < numBlockTypes; i++)
                {
                    uint len = ReadUInt32E(br, isLittleEndian);
                    blockTypes[i] = Encoding.UTF8.GetString(br.ReadBytes((int)len));
                }

                ushort[] blockTypeIndices = new ushort[numBlocks];
                for (int i = 0; i < numBlocks; i++)
                {
                    blockTypeIndices[i] = ReadUInt16E(br, isLittleEndian);
                }

                uint[] blockSizes = new uint[numBlocks];
                for (int i = 0; i < numBlocks; i++)
                {
                    blockSizes[i] = ReadUInt32E(br, isLittleEndian);
                }

                uint numStrings = ReadUInt32E(br, isLittleEndian);
                uint maxStringLen = ReadUInt32E(br, isLittleEndian);

                for (int i = 0; i < numStrings; i++)
                {
                    uint len = ReadUInt32E(br, isLittleEndian);
                    ms.Position += len;
                }

                uint numGroups = ReadUInt32E(br, isLittleEndian);
                for (int i = 0; i < numGroups; i++)
                {
                    ms.Position += 4;
                }

                if (numBlocks != 1 || !blockTypes[blockTypeIndices[0]].Contains("xml::dom::CStreamableNode"))
                {
                    log($"File does not contain a single xml::dom::CStreamableNode block. Found {numBlocks} blocks, type: {blockTypes[blockTypeIndices[0]]}");
                    return false;
                }

                uint nodeCount = ReadUInt32E(br, isLittleEndian);
                uint totalAttrCount = ReadUInt32E(br, isLittleEndian);
                uint stringBlockSize = ReadUInt32E(br, isLittleEndian);

                byte[] stringBlock = br.ReadBytes((int)stringBlockSize);
                int strOffset = 0;

                string GetNextString()
                {
                    int end = strOffset;
                    while (end < stringBlock.Length && stringBlock[end] != 0)
                    {
                        end++;
                    }

                    string s = Encoding.UTF8.GetString(stringBlock, strOffset, end - strOffset);
                    strOffset = end + 1;

                    // Sanitize string for XML
                    if (s.Length > 0)
                    {
                        var sb = new StringBuilder(s.Length);
                        foreach (char c in s)
                        {
                            bool isXmlChar = c == 0x9 || c == 0xA || c == 0xD || (c >= 0x20 && c <= 0xD7FF) || (c >= 0xE000 && c <= 0xFFFD);
                            if (isXmlChar)
                            {
                                sb.Append(c);
                            }
                        }
                        s = sb.ToString();
                        if (string.IsNullOrWhiteSpace(s))
                        {
                            s = "";
                        }
                    }
                    return s;
                }

                GamebryoNode ReadNode()
                {
                    var node = new GamebryoNode();
                    byte flags = br.ReadByte();
                    node.Data = ReadUInt32E(br, isLittleEndian);

                    if ((flags & 0x04) != 0)
                    {
                        node.Value = GetNextString();
                    }

                    if ((flags & 0x02) != 0)
                    {
                        uint attrCount = ((flags & 0x08) == 0) ? ReadUInt32E(br, isLittleEndian) : br.ReadByte();
                        for (int i = 0; i < attrCount; i++)
                        {
                            node.Attributes.Add(new GamebryoAttribute
                            {
                                Data = ReadUInt32E(br, isLittleEndian),
                                Value = GetNextString()
                            });
                        }
                    }

                    if ((flags & 0x01) != 0)
                    {
                        uint childCount = ((flags & 0x08) == 0) ? ReadUInt32E(br, isLittleEndian) : br.ReadByte();
                        for (int i = 0; i < childCount; i++)
                        {
                            node.Children.Add(ReadNode());
                        }
                    }

                    return node;
                }

                var rootNode = ReadNode();

                XElement ToXml(GamebryoNode node)
                {
                    var el = new XElement("Node");
                    if (_hashToString.TryGetValue(node.Data, out string? name))
                    {
                        el.Add(new XAttribute("Name", name));
                    }
                    else
                    {
                        el.Add(new XAttribute("Data", node.Data.ToString("X8")));
                    }

                    if (node.Value != null)
                    {
                        el.Add(new XElement("Value", node.Value));
                    }
                    if (node.Attributes.Count > 0)
                    {
                        var attrsEl = new XElement("Attributes");
                        foreach (var a in node.Attributes)
                        {
                            var attrEl = new XElement("Attr", new XAttribute("Value", a.Value));
                            if (_hashToString.TryGetValue(a.Data, out string? aName))
                            {
                                attrEl.Add(new XAttribute("Name", aName));
                            }
                            else
                            {
                                attrEl.Add(new XAttribute("Data", a.Data.ToString("X8")));
                            }
                            attrsEl.Add(attrEl);
                        }
                        el.Add(attrsEl);
                    }
                    if (node.Children.Count > 0)
                    {
                        var childrenEl = new XElement("Children");
                        foreach (var c in node.Children)
                        {
                            childrenEl.Add(ToXml(c));
                        }
                        el.Add(childrenEl);
                    }
                    return el;
                }

                var xmlDoc = new XDocument(new XDeclaration("1.0", "utf-8", "yes"), ToXml(rootNode));
                xmlDoc.Save(xmlPath);

                log($"Successfully extracted to readable XML: {Path.GetFileName(xmlPath)}");
                return true;
            }
            catch (Exception ex)
            {
                log($"Extraction failed: {ex.Message}");
                return false;
            }
        }

        public static async Task<bool> RepackRealXmlToBinaryXmlAsync(string originalNifPath, string xmlPath, string outNifPath, Action<string> log)
        {
            try
            {
                XDocument xmlDoc = XDocument.Load(xmlPath, LoadOptions.PreserveWhitespace);
                if (xmlDoc.Root == null)
                {
                    throw new Exception("Invalid XML: No root element.");
                }

                GamebryoNode FromXml(XElement el)
                {
                    uint dataVal = 0;
                    var nameAttr = el.Attribute("Name");
                    var dataAttr = el.Attribute("Data");

                    if (nameAttr != null)
                    {
                        dataVal = GetHashValue(nameAttr.Value);
                    }
                    else if (dataAttr != null)
                    {
                        string dataStr = dataAttr.Value;
                        dataVal = dataStr.StartsWith("0x", StringComparison.OrdinalIgnoreCase)
                            ? Convert.ToUInt32(dataStr, 16)
                            : (uint.TryParse(dataStr, System.Globalization.NumberStyles.HexNumber, null, out uint hexVal) ? hexVal : uint.Parse(dataStr));
                    }

                    var node = new GamebryoNode
                    {
                        Data = dataVal
                    };

                    var valEl = el.Element("Value");
                    if (valEl != null)
                    {
                        node.Value = valEl.Value;
                        if (string.IsNullOrWhiteSpace(node.Value))
                        {
                            node.Value = "";
                        }
                    }

                    var attrsEl = el.Element("Attributes");
                    if (attrsEl != null)
                    {
                        foreach (var aEl in attrsEl.Elements("Attr"))
                        {
                            string attrVal = aEl.Attribute("Value")!.Value;
                            if (string.IsNullOrWhiteSpace(attrVal))
                            {
                                attrVal = "";
                            }

                            uint attrDataVal = 0;
                            var aNameAttr = aEl.Attribute("Name");
                            var aDataAttr = aEl.Attribute("Data");

                            if (aNameAttr != null)
                            {
                                attrDataVal = GetHashValue(aNameAttr.Value);
                            }
                            else if (aDataAttr != null)
                            {
                                string attrDataStr = aDataAttr.Value;
                                attrDataVal = attrDataStr.StartsWith("0x", StringComparison.OrdinalIgnoreCase)
                                    ? Convert.ToUInt32(attrDataStr, 16)
                                    : (uint.TryParse(attrDataStr, System.Globalization.NumberStyles.HexNumber, null, out uint aHexVal) ? aHexVal : uint.Parse(attrDataStr));
                            }

                            node.Attributes.Add(new GamebryoAttribute
                            {
                                Data = attrDataVal,
                                Value = attrVal
                            });
                        }
                    }

                    var childrenEl = el.Element("Children");
                    if (childrenEl != null)
                    {
                        foreach (var cEl in childrenEl.Elements("Node"))
                        {
                            node.Children.Add(FromXml(cEl));
                        }
                    }
                    return node;
                }

                var rootNode = FromXml(xmlDoc.Root);

                // Read original NIF header to determine endianness
                byte[] data = await File.ReadAllBytesAsync(originalNifPath);
                using var ms = new MemoryStream(data);
                using var br = new BinaryReader(ms);

                string headerStr = "";
                while (ms.Position < ms.Length)
                {
                    char c = br.ReadChar();
                    headerStr += c;
                    if (c == '\n')
                    {
                        break;
                    }
                }

                if (!headerStr.StartsWith("Gamebryo File Format"))
                {
                    log("Original file is not a valid Gamebryo file.");
                    return false;
                }

                uint version = br.ReadUInt32();
                byte endian = br.ReadByte();
                bool isLittleEndian = endian != 0;

                uint userVersion = ReadUInt32E(br, isLittleEndian);
                uint numBlocks = ReadUInt32E(br, isLittleEndian);
                ushort numBlockTypes = ReadUInt16E(br, isLittleEndian);

                for (int i = 0; i < numBlockTypes; i++)
                {
                    uint len = ReadUInt32E(br, isLittleEndian);
                    ms.Position += len;
                }

                for (int i = 0; i < numBlocks; i++)
                {
                    ReadUInt16E(br, isLittleEndian);
                }

                long blockSizeOffset = ms.Position;
                uint[] blockSizes = new uint[numBlocks];
                for (int i = 0; i < numBlocks; i++)
                {
                    blockSizes[i] = ReadUInt32E(br, isLittleEndian);
                }

                uint numStrings = ReadUInt32E(br, isLittleEndian);
                uint maxStringLen = ReadUInt32E(br, isLittleEndian);

                for (int i = 0; i < numStrings; i++)
                {
                    uint len = ReadUInt32E(br, isLittleEndian);
                    ms.Position += len;
                }

                uint numGroups = ReadUInt32E(br, isLittleEndian);
                for (int i = 0; i < numGroups; i++)
                {
                    ms.Position += 4;
                }

                long afterHeaderOffset = ms.Position;

                uint totalNodes = 0;
                uint totalAttrs = 0;
                List<byte> stringBlock = [];
                List<byte> nodeDataBlock = [];

                void WriteNode(GamebryoNode node, BinaryWriter nw)
                {
                    totalNodes++;
                    byte flags = 0;
                    if (node.Children.Count > 0)
                    {
                        flags |= 0x01;
                    }

                    if (node.Attributes.Count > 0)
                    {
                        flags |= 0x02;
                    }

                    if (node.Value != null)
                    {
                        flags |= 0x04;
                    }

                    bool compress = (node.Children.Count <= 255 && node.Attributes.Count <= 255);
                    if (compress)
                    {
                        flags |= 0x08;
                    }

                    nw.Write(flags);
                    WriteUInt32E(nw, node.Data, isLittleEndian);

                    if (node.Value != null)
                    {
                        stringBlock.AddRange(Encoding.UTF8.GetBytes(node.Value));
                        stringBlock.Add(0);
                    }

                    if (node.Attributes.Count > 0)
                    {
                        if (compress)
                        {
                            nw.Write((byte)node.Attributes.Count);
                        }
                        else
                        {
                            WriteUInt32E(nw, (uint)node.Attributes.Count, isLittleEndian);
                        }

                        foreach (var attr in node.Attributes)
                        {
                            totalAttrs++;
                            WriteUInt32E(nw, attr.Data, isLittleEndian);
                            stringBlock.AddRange(Encoding.UTF8.GetBytes(attr.Value ?? ""));
                            stringBlock.Add(0);
                        }
                    }

                    if (node.Children.Count > 0)
                    {
                        if (compress)
                        {
                            nw.Write((byte)node.Children.Count);
                        }
                        else
                        {
                            WriteUInt32E(nw, (uint)node.Children.Count, isLittleEndian);
                        }

                        foreach (var child in node.Children)
                        {
                            WriteNode(child, nw);
                        }
                    }
                }

                using var nodeDataMs = new MemoryStream();
                using var nodeDataBw = new BinaryWriter(nodeDataMs);
                WriteNode(rootNode, nodeDataBw);
                nodeDataBlock.AddRange(nodeDataMs.ToArray());

                uint newBlockSize = 12 + (uint)stringBlock.Count + (uint)nodeDataBlock.Count;

                using var outFs = File.Create(outNifPath);
                using var outBw = new BinaryWriter(outFs);

                outBw.Write(data, 0, (int)blockSizeOffset);
                WriteUInt32E(outBw, newBlockSize, isLittleEndian);

                int afterBlockSizesOffset = (int)(blockSizeOffset + 4);
                int lenToCopy = (int)(afterHeaderOffset - afterBlockSizesOffset);
                outBw.Write(data, afterBlockSizesOffset, lenToCopy);

                WriteUInt32E(outBw, totalNodes, isLittleEndian);
                WriteUInt32E(outBw, totalAttrs, isLittleEndian);
                WriteUInt32E(outBw, (uint)stringBlock.Count, isLittleEndian);
                outBw.Write(stringBlock.ToArray());
                outBw.Write(nodeDataBlock.ToArray());

                // Copy NIF footer (anything after the blocks)
                long footerOffset = afterHeaderOffset;
                for (int i = 0; i < numBlocks; i++)
                {
                    footerOffset += blockSizes[i];
                }
                if (footerOffset < data.Length)
                {
                    outBw.Write(data, (int)footerOffset, (int)(data.Length - footerOffset));
                }

                log($"Successfully repacked into binary NIF: {Path.GetFileName(outNifPath)}");
                return true;
            }
            catch (Exception ex)
            {
                log($"Repack failed: {ex.Message}");
                return false;
            }
        }
    }
}
