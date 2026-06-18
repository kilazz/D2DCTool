import os
import sys


def main():
    if len(sys.argv) < 2:
        print("Usage: python keep_only_xml.py <folder_path>")
        print('Example: python keep_only_xml.py "C:\\MyGame\\Unpacked"')
        return

    target_dir = sys.argv[1]

    if not os.path.isdir(target_dir):
        print(f"Error: Folder not found -> {target_dir}")
        return

    print("!!! WARNING !!!")
    print(
        "This script will PERMANENTLY delete ALL files (except .xml) and EMPTY FOLDERS in:"
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

    # os.walk with topdown=False goes bottom-up (deepest nested folders first, then parents)
    # This is needed to correctly delete folders that became empty after file deletion
    for root, dirs, files in os.walk(target_dir, topdown=False):
        # 1. Delete unnecessary files
        for file in files:
            file_path = os.path.join(root, file)

            if not file.lower().endswith(".xml"):
                try:
                    os.remove(file_path)
                    deleted_files += 1
                except Exception as e:
                    print(f"Failed to delete file {file_path}: {e}")
            else:
                kept_files += 1

        # 2. Check if folder became empty (and is not the root folder we passed)
        if root != target_dir:
            try:
                if not os.listdir(root):  # If list of files and folders inside is empty
                    os.rmdir(root)
                    deleted_dirs += 1
            except Exception as e:
                print(f"Failed to delete folder {root}: {e}")

    print("\nDone!")
    print(f"Deleted junk files: {deleted_files}")
    print(f"Deleted empty folders: {deleted_dirs}")
    print(f"Kept .xml files: {kept_files}")


if __name__ == "__main__":
    main()
