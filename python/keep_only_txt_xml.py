import os
import sys


def main():
    if len(sys.argv) < 2:
        print("Usage: python keep_only_txt_xml.py <folder_path>")
        print('Example: python keep_only_txt_xml.py "C:\\MyGame\\Unpacked"')
        return

    target_dir = sys.argv[1]

    if not os.path.isdir(target_dir):
        print(f"Error: Folder not found -> {target_dir}")
        return

    print("!!! WARNING !!!")
    print(
        "This script will PERMANENTLY delete ALL .xml files (EXCEPT .txt.xml) and EMPTY FOLDERS in:"
    )
    print(f"-> {target_dir}")

    confirm = input("Are you absolutely sure? (type 'y' to confirm): ")
    if confirm.lower() != "y":
        print("Operation cancelled.")
        return

    deleted_files = 0
    kept_files = 0
    deleted_dirs = 0

    print("\nCleanup started...")

    # os.walk with topdown=False goes bottom-up
    for root, dirs, files in os.walk(target_dir, topdown=False):
        # 1. Delete unnecessary files
        for file in files:
            file_path = os.path.join(root, file)
            lower_file = file.lower()

            # If file ends with .xml but NOT with .txt.xml
            if lower_file.endswith(".xml") and not lower_file.endswith(".txt.xml"):
                try:
                    os.remove(file_path)
                    deleted_files += 1
                except Exception as e:
                    print(f"Failed to delete file {file_path}: {e}")
            elif lower_file.endswith(".txt.xml"):
                kept_files += 1

        # 2. Check if folder became empty
        if root != target_dir:
            try:
                if not os.listdir(root):  # If list of files and folders inside is empty
                    os.rmdir(root)
                    deleted_dirs += 1
            except Exception as e:
                print(f"Failed to delete folder {root}: {e}")

    print("\nDone!")
    print(f"Deleted normal .xml files: {deleted_files}")
    print(f"Deleted empty folders: {deleted_dirs}")
    print(f"Kept .txt.xml files: {kept_files}")


if __name__ == "__main__":
    main()
