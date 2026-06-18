import sys
import os


def get_hash(text):
    """Gamebryo hashing algorithm"""
    h = 0
    for c in text:
        h = (h * 0x21 + ord(c)) & 0xFFFFFFFF
    return h


def main():
    if len(sys.argv) < 4:
        print(
            "Usage: python hash_cracker.py <strings.txt> <unknown_hashes.txt> <output.txt>"
        )
        print("Example: python hash_cracker.py strings.txt hashes.txt found.txt")
        return

    strings_file = sys.argv[1]
    hashes_file = sys.argv[2]
    output_file = sys.argv[3]

    if not os.path.exists(hashes_file):
        print(f"Error: Hashes file not found -> {hashes_file}")
        return

    if not os.path.exists(strings_file):
        print(f"Error: Strings dictionary file not found -> {strings_file}")
        return

    # 1. Load unknown hashes
    unknown_hashes = set()
    with open(hashes_file, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                # Read hex string (e.g., "00000DF6") to integer
                unknown_hashes.add(int(line, 16))
            except ValueError:
                pass

    print(f"Loaded unknown hashes: {len(unknown_hashes)}")

    # 2. Read dictionary and find matches
    found_tags = {}
    print("Reading dictionary and calculating hashes (this may take some time)...")

    with open(strings_file, "r", encoding="utf-8", errors="ignore") as f:
        for i, line in enumerate(f):
            word = line.strip()
            if not word:
                continue

            h = get_hash(word)

            if h in unknown_hashes and h not in found_tags:
                found_tags[h] = word
                print(f"[FOUND] {h:08X} -> {word}")

            if (i + 1) % 1000000 == 0:
                print(f"Processed {i + 1} lines...")

    # Calculate which hashes remain unfound
    not_found_hashes = unknown_hashes - set(found_tags.keys())

    print("\nSearch completed.")
    print(f"Found: {len(found_tags)} out of {len(unknown_hashes)}")
    print(f"Not found: {len(not_found_hashes)}")

    # 3. Save results
    with open(output_file, "w", encoding="utf-8") as f:
        f.write("--- Found tags ---\n")
        for h, w in sorted(found_tags.items()):
            f.write(f"{h:08X} = {w}\n")

        f.write("\n--- Unfound hashes (remain unknown) ---\n")
        for h in sorted(not_found_hashes):
            f.write(f"{h:08X}\n")

        f.write("\n--- Simple word list (to add to tags.txt) ---\n")
        for w in sorted(found_tags.values()):
            f.write(f"{w}\n")

    print(f"Results saved to file: {output_file}")


if __name__ == "__main__":
    main()
