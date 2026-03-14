using System;
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
                    if (c == '\n') break;
                }

                if (!headerStr.StartsWith("Gamebryo File Format"))
                {
                    log("Not a valid Gamebryo file.");
                    return false;
                }

                uint version = br.ReadUInt32();
                byte endian = br.ReadByte();
                uint userVersion = br.ReadUInt32();
                uint numBlocks = br.ReadUInt32();
                ushort numBlockTypes = br.ReadUInt16();

                string[] blockTypes = new string[numBlockTypes];
                for (int i = 0; i < numBlockTypes; i++)
                {
                    uint len = br.ReadUInt32();
                    blockTypes[i] = Encoding.UTF8.GetString(br.ReadBytes((int)len));
                }

                ushort[] blockTypeIndices = new ushort[numBlocks];
                for (int i = 0; i < numBlocks; i++)
                {
                    blockTypeIndices[i] = br.ReadUInt16();
                }

                uint[] blockSizes = new uint[numBlocks];
                for (int i = 0; i < numBlocks; i++)
                {
                    blockSizes[i] = br.ReadUInt32();
                }

                uint numStrings = br.ReadUInt32();
                uint maxStringLen = br.ReadUInt32();

                for (int i = 0; i < numStrings; i++)
                {
                    uint len = br.ReadUInt32();
                    ms.Position += len;
                }

                uint numGroups = br.ReadUInt32();
                for (int i = 0; i < numGroups; i++)
                {
                    ms.Position += 4;
                }

                if (numBlocks != 1 || !blockTypes[blockTypeIndices[0]].Contains("xml::dom::CStreamableNode"))
                {
                    log($"File does not contain a single xml::dom::CStreamableNode block. Found {numBlocks} blocks, type: {blockTypes[blockTypeIndices[0]]}");
                    return false;
                }

                uint nodeCount = br.ReadUInt32();
                uint totalAttrCount = br.ReadUInt32();
                uint stringBlockSize = br.ReadUInt32();

                byte[] stringBlock = br.ReadBytes((int)stringBlockSize);
                int strOffset = 0;

                string GetNextString()
                {
                    int end = strOffset;
                    while (end < stringBlock.Length && stringBlock[end] != 0) end++;
                    string s = Encoding.UTF8.GetString(stringBlock, strOffset, end - strOffset);
                    strOffset = end + 1;

                    // Sanitize string for XML
                    if (s.Length > 0)
                    {
                        var sb = new StringBuilder(s.Length);
                        foreach (char c in s)
                        {
                            bool isXmlChar = c == 0x9 || c == 0xA || c == 0xD || (c >= 0x20 && c <= 0xD7FF) || (c >= 0xE000 && c <= 0xFFFD);
                            if (isXmlChar) sb.Append(c);
                        }
                        s = sb.ToString();
                        if (string.IsNullOrWhiteSpace(s)) s = "";
                    }
                    return s;
                }

                GamebryoNode ReadNode()
                {
                    var node = new GamebryoNode();
                    byte flags = br.ReadByte();
                    node.Data = br.ReadUInt32();

                    if ((flags & 0x04) != 0)
                    {
                        node.Value = GetNextString();
                    }

                    if ((flags & 0x02) != 0)
                    {
                        uint attrCount = ((flags & 0x08) == 0) ? br.ReadUInt32() : br.ReadByte();
                        for (int i = 0; i < attrCount; i++)
                        {
                            node.Attributes.Add(new GamebryoAttribute
                            {
                                Data = br.ReadUInt32(),
                                Value = GetNextString()
                            });
                        }
                    }

                    if ((flags & 0x01) != 0)
                    {
                        uint childCount = ((flags & 0x08) == 0) ? br.ReadUInt32() : br.ReadByte();
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
                    var el = new XElement("Node", new XAttribute("Data", node.Data));
                    if (node.Value != null)
                    {
                        el.Add(new XElement("Value", node.Value));
                    }
                    if (node.Attributes.Count > 0)
                    {
                        var attrsEl = new XElement("Attributes");
                        foreach (var a in node.Attributes)
                        {
                            attrsEl.Add(new XElement("Attr", new XAttribute("Data", a.Data), new XAttribute("Value", a.Value)));
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
                if (xmlDoc.Root == null) throw new Exception("Invalid XML: No root element.");

                GamebryoNode FromXml(XElement el)
                {
                    var node = new GamebryoNode
                    {
                        Data = uint.Parse(el.Attribute("Data")!.Value)
                    };

                    var valEl = el.Element("Value");
                    if (valEl != null)
                    {
                        node.Value = valEl.Value;
                        if (string.IsNullOrWhiteSpace(node.Value)) node.Value = "";
                    }

                    var attrsEl = el.Element("Attributes");
                    if (attrsEl != null)
                    {
                        foreach (var aEl in attrsEl.Elements("Attr"))
                        {
                            string attrVal = aEl.Attribute("Value")!.Value;
                            if (string.IsNullOrWhiteSpace(attrVal)) attrVal = "";

                            node.Attributes.Add(new GamebryoAttribute
                            {
                                Data = uint.Parse(aEl.Attribute("Data")!.Value),
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

                uint totalNodes = 0;
                uint totalAttrs = 0;
                List<byte> stringBlock = [];
                List<byte> nodeDataBlock = [];

                void WriteNode(GamebryoNode node, BinaryWriter nw)
                {
                    totalNodes++;
                    byte flags = 0;
                    if (node.Children.Count > 0) flags |= 0x01;
                    if (node.Attributes.Count > 0) flags |= 0x02;
                    if (node.Value != null) flags |= 0x04;

                    bool compress = (node.Children.Count <= 255 && node.Attributes.Count <= 255);
                    if (compress) flags |= 0x08;

                    nw.Write(flags);
                    nw.Write(node.Data);

                    if (node.Value != null)
                    {
                        stringBlock.AddRange(Encoding.UTF8.GetBytes(node.Value));
                        stringBlock.Add(0);
                    }

                    if (node.Attributes.Count > 0)
                    {
                        if (compress) nw.Write((byte)node.Attributes.Count);
                        else nw.Write((uint)node.Attributes.Count);

                        foreach (var attr in node.Attributes)
                        {
                            totalAttrs++;
                            nw.Write(attr.Data);
                            stringBlock.AddRange(Encoding.UTF8.GetBytes(attr.Value ?? ""));
                            stringBlock.Add(0);
                        }
                    }

                    if (node.Children.Count > 0)
                    {
                        if (compress) nw.Write((byte)node.Children.Count);
                        else nw.Write((uint)node.Children.Count);

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

                // Read original NIF header
                byte[] data = await File.ReadAllBytesAsync(originalNifPath);
                using var ms = new MemoryStream(data);
                using var br = new BinaryReader(ms);

                string headerStr = "";
                while (ms.Position < ms.Length)
                {
                    char c = br.ReadChar();
                    headerStr += c;
                    if (c == '\n') break;
                }

                if (!headerStr.StartsWith("Gamebryo File Format"))
                {
                    log("Original file is not a valid Gamebryo file.");
                    return false;
                }

                uint version = br.ReadUInt32();
                byte endian = br.ReadByte();
                uint userVersion = br.ReadUInt32();
                uint numBlocks = br.ReadUInt32();
                ushort numBlockTypes = br.ReadUInt16();

                for (int i = 0; i < numBlockTypes; i++)
                {
                    uint len = br.ReadUInt32();
                    ms.Position += len;
                }

                for (int i = 0; i < numBlocks; i++)
                {
                    br.ReadUInt16();
                }

                long blockSizeOffset = ms.Position;
                uint[] blockSizes = new uint[numBlocks];
                for (int i = 0; i < numBlocks; i++)
                {
                    blockSizes[i] = br.ReadUInt32();
                }

                uint numStrings = br.ReadUInt32();
                uint maxStringLen = br.ReadUInt32();

                for (int i = 0; i < numStrings; i++)
                {
                    uint len = br.ReadUInt32();
                    ms.Position += len;
                }

                uint numGroups = br.ReadUInt32();
                for (int i = 0; i < numGroups; i++)
                {
                    ms.Position += 4;
                }

                long afterHeaderOffset = ms.Position;

                uint newBlockSize = 12 + (uint)stringBlock.Count + (uint)nodeDataBlock.Count;

                using var outFs = File.Create(outNifPath);
                using var outBw = new BinaryWriter(outFs);

                outBw.Write(data, 0, (int)blockSizeOffset);
                outBw.Write(newBlockSize);

                int afterBlockSizesOffset = (int)(blockSizeOffset + 4);
                int lenToCopy = (int)(afterHeaderOffset - afterBlockSizesOffset);
                outBw.Write(data, afterBlockSizesOffset, lenToCopy);

                outBw.Write(totalNodes);
                outBw.Write(totalAttrs);
                outBw.Write((uint)stringBlock.Count);
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
