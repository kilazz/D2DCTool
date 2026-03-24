import os
import sys
import re
import struct

def get_hash(text):
    """Gamebryo hashing algorithm"""
    h = 0
    for c in text:
        h = (h * 0x21 + ord(c)) & 0xFFFFFFFF
    return h

def parse_binary_xml_for_hashes(filepath):
    """Parses binary Gamebryo XML and extracts all hashes (Data)"""
    hashes = set()
    try:
        with open(filepath, 'rb') as f:
            # Read header
            header = b""
            while True:
                c = f.read(1)
                if not c or c == b'\n':
                    break
                header += c

            if not header.startswith(b"Gamebryo File Format"):
                return None # Not a binary XML

            version = struct.unpack('<I', f.read(4))[0]
            endian = f.read(1)[0]
            is_le = (endian != 0)
            endian_char = '<' if is_le else '>'

            def read_u32():
                return struct.unpack(endian_char + 'I', f.read(4))[0]
            def read_u16():
                return struct.unpack(endian_char + 'H', f.read(2))[0]
            def read_u8():
                return f.read(1)[0]

            userVersion = read_u32()
            numBlocks = read_u32()
            numBlockTypes = read_u16()

            blockTypes = []
            for _ in range(numBlockTypes):
                length = read_u32()
                blockTypes.append(f.read(length))

            blockTypeIndices = [read_u16() for _ in range(numBlocks)]
            blockSizes = [read_u32() for _ in range(numBlocks)]

            numStrings = read_u32()
            maxStringLen = read_u32()
            for _ in range(numStrings):
                length = read_u32()
                f.seek(length, 1)

            numGroups = read_u32()
            for _ in range(numGroups):
                f.seek(4, 1)

            if numBlocks != 1 or b"xml::dom::CStreamableNode" not in blockTypes[blockTypeIndices[0]]:
                return hashes

            nodeCount = read_u32()
            totalAttrCount = read_u32()
            stringBlockSize = read_u32()

            # Skip string block, we only need hashes (Data)
            f.seek(stringBlockSize, 1)

            # Recursive node reading
            def read_node():
                flags = read_u8()
                data = read_u32()
                hashes.add(data)

                if flags & 0x02: # Has attributes
                    attrCount = read_u32() if (flags & 0x08) == 0 else read_u8()
                    for _ in range(attrCount):
                        attr_data = read_u32()
                        hashes.add(attr_data)

                if flags & 0x01: # Has child nodes
                    childCount = read_u32() if (flags & 0x08) == 0 else read_u8()
                    for _ in range(childCount):
                        read_node()

            read_node()
            return hashes
    except Exception as e:
        return None

def parse_text_xml_for_hashes(filepath):
    """Fallback method: searches for hashes in text XML via regex"""
    hashes = set()
    pattern = re.compile(r'\b(?:Name|Data)="([0-9A-Fa-f]{8})"')
    try:
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            content = f.read()
            matches = pattern.findall(content)
            for match in matches:
                hashes.add(int(match, 16))
    except Exception:
        pass
    return hashes

def main():
    if len(sys.argv) < 2:
        print("Usage: python scan_binary_xml_hashes.py <xml_folder>")
        print("Example: python scan_binary_xml_hashes.py \"C:\\Games\\Divinity2\\Data\\Public\"")
        return

    xml_dir = sys.argv[1]

    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    dict_file = os.path.join(base_dir, "hash_dictionary.txt")
    out_file = os.path.join(base_dir, "unknown_hashes.txt")

    if not os.path.isdir(xml_dir):
        print(f"Error: Folder not found -> {xml_dir}")
        return

    # 1. Read and clean dictionary
    known_strings = set()
    if os.path.exists(dict_file):
        with open(dict_file, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                s = line.strip()
                if s:
                    known_strings.add(s)

    print(f"Calculating hashes for {len(known_strings)} known strings...")
    known_hashes = {get_hash(s) for s in known_strings}

    # 2. Scan files
    all_found_hashes = set()
    binary_count = 0
    text_count = 0

    print(f"Scanning folder: {xml_dir} ...")
    for root, dirs, files in os.walk(xml_dir):
        for file in files:
            if file.lower().endswith('.xml'):
                file_path = os.path.join(root, file)

                # Try as binary first
                hashes = parse_binary_xml_for_hashes(file_path)
                if hashes is not None:
                    all_found_hashes.update(hashes)
                    binary_count += 1
                else:
                    # If not binary, read as text
                    hashes = parse_text_xml_for_hashes(file_path)
                    all_found_hashes.update(hashes)
                    text_count += 1

    print("\n--- SCAN RESULTS ---")
    print(f"Processed binary XMLs: {binary_count}")
    print(f"Processed text XMLs: {text_count}")
    print(f"Total unique hashes found in files: {len(all_found_hashes)}")

    # 3. Filter unknown
    unknown_hashes = all_found_hashes - known_hashes
    known_found = all_found_hashes.intersection(known_hashes)

    print(f"Of those, ALREADY KNOWN (in dictionary): {len(known_found)}")
    print(f"REMAINING UNKNOWN: {len(unknown_hashes)}")

    # 4. Rewrite cleaned dictionary
    with open(dict_file, 'w', encoding='utf-8') as f:
        for s in sorted(known_strings):
            f.write(f"{s}\n")
    print(f"\n[OK] Cleaned dictionary saved to: {dict_file}")

    # 5. Save unknown hashes
    with open(out_file, 'w', encoding='utf-8') as f:
        for h in sorted(unknown_hashes):
            f.write(f"{h:08X}\n")
    print(f"[OK] Clean list of unknown hashes saved to: {out_file}")

if __name__ == "__main__":
    main()
