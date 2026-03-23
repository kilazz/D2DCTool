import os
import re
import sys

def main():
    if len(sys.argv) < 3:
        print("Usage: python extract_hashes.py <xml_folder> <output_hashes.txt>")
        print("Example: python extract_hashes.py ./xml_files unknown_hashes.txt")
        return

    input_dir = sys.argv[1]
    output_file = sys.argv[2]

    if not os.path.isdir(input_dir):
        print(f"Error: Folder not found -> {input_dir}")
        return

    # Regular expression to find 8-character hexadecimal strings in Name or Data attributes
    # Example: Name="00000DF6" or Data="4A8B9C12"
    pattern = re.compile(r'\b(?:Name|Data)="([0-9A-Fa-f]{8})"')

    unique_hashes = set()
    file_count = 0

    print(f"Scanning folder: {input_dir}")

    # Recursively traverse all files in the folder and subfolders
    for root, dirs, files in os.walk(input_dir):
        for file in files:
            if file.lower().endswith('.xml'):
                file_path = os.path.join(root, file)
                file_count += 1
                try:
                    with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                        content = f.read()
                        matches = pattern.findall(content)
                        for match in matches:
                            # Add to set (to exclude duplicates)
                            unique_hashes.add(match.upper())
                except Exception as e:
                    print(f"Error reading file {file_path}: {e}")

    print(f"Scanned XML files: {file_count}")
    print(f"Found unique unknown hashes: {len(unique_hashes)}")

    # Save found hashes to file
    with open(output_file, 'w', encoding='utf-8') as f:
        for h in sorted(unique_hashes):
            f.write(f"{h}\n")

    print(f"Hashes saved to file: {output_file}")

if __name__ == "__main__":
    main()
