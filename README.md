# D2DCTool
A cross-platform utility for **Divinity 2 Developer's Cut** modding.

## Features
*   **Archive Tools (.dv2):** Extract and pack game archives with Zlib compression support.
*   **Texture Tools:** Convert between DDS and Gamebryo NIF formats (supports batch processing).
*   **XML Tools:** Extract binary Gamebryo XML to readable text and repack edited files (supports batch processing).

## Usage
1.  Run the application.
2.  Select the desired tool (Archive, Texture, or XML).
3.  Choose your source file or folder.
4.  Execute the operation and monitor progress in the log panel.

## Requirements
*   **.NET 10 SDK**

## Build
```bash
dotnet run --project D2DCTool/D2DCTool.csproj
