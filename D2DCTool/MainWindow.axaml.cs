using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.IO.Compression;

namespace D2DCTool;

public partial class MainWindow : Window
{
    private string? _currentDv2Path;
    private string? _currentFolderPath;
    private readonly ObservableCollection<FileTreeNode> _treeItems = [];
    private static readonly FilePickerFileType[] BinaryXmlFileType = [new("Binary XML Files") { Patterns = ["*.xml"] }];
    private static readonly FilePickerFileType[] ReadableXmlFileType = [new("Readable XML Files") { Patterns = ["*.xml"] }];
    private static readonly FilePickerFileType[] DdsFileType = [new("DDS Textures") { Patterns = ["*.dds"] }];
    private static readonly FilePickerFileType[] NifFileType = [new("NIF Models") { Patterns = ["*.nif"] }];
    private static readonly FilePickerFileType[] Dv2FileType = [new("Divinity 2 Archive") { Patterns = ["*.dv2"] }];

    public MainWindow()
    {
        InitializeComponent();

        var btnOpenDv2 = this.FindControl<Button>("BtnOpenDv2");
        var btnUnpack = this.FindControl<Button>("BtnUnpack");
        var btnBatchUnpack = this.FindControl<Button>("BtnBatchUnpack");
        var btnOpenFolder = this.FindControl<Button>("BtnOpenFolder");
        var btnPack = this.FindControl<Button>("BtnPack");
        var fileTree = this.FindControl<TreeView>("FileTree");

        if (btnOpenDv2 != null) btnOpenDv2.Click += BtnOpenDv2_Click;
        if (btnUnpack != null) btnUnpack.Click += BtnUnpack_Click;
        if (btnBatchUnpack != null) btnBatchUnpack.Click += BtnBatchUnpack_Click;
        if (btnOpenFolder != null) btnOpenFolder.Click += BtnOpenFolder_Click;
        if (btnPack != null) btnPack.Click += BtnPack_Click;

        var btnConvertDds = this.FindControl<Button>("BtnConvertDds");
        if (btnConvertDds != null) btnConvertDds.Click += BtnConvertDds_Click;

        var btnExtractNif = this.FindControl<Button>("BtnExtractNif");
        if (btnExtractNif != null) btnExtractNif.Click += BtnExtractNif_Click;

        var btnBatchDdsToNif = this.FindControl<Button>("BtnBatchDdsToNif");
        if (btnBatchDdsToNif != null) btnBatchDdsToNif.Click += BtnBatchDdsToNif_Click;

        var btnBatchNifToDds = this.FindControl<Button>("BtnBatchNifToDds");
        if (btnBatchNifToDds != null) btnBatchNifToDds.Click += BtnBatchNifToDds_Click;

        var btnExtractXml = this.FindControl<Button>("BtnExtractXml");
        if (btnExtractXml != null) btnExtractXml.Click += BtnExtractXml_Click;

        var btnRepackXml = this.FindControl<Button>("BtnRepackXml");
        if (btnRepackXml != null) btnRepackXml.Click += BtnRepackXml_Click;

        var btnBatchExtractXml = this.FindControl<Button>("BtnBatchExtractXml");
        if (btnBatchExtractXml != null) btnBatchExtractXml.Click += BtnBatchExtractXml_Click;

        var btnBatchRepackXml = this.FindControl<Button>("BtnBatchRepackXml");
        if (btnBatchRepackXml != null) btnBatchRepackXml.Click += BtnBatchRepackXml_Click;

        if (fileTree != null) fileTree.ItemsSource = _treeItems;
    }

    private async void BtnExtractXml_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var files = await topLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select Gamebryo .xml file to extract",
            AllowMultiple = false,
            FileTypeFilter = new[] { new FilePickerFileType("Binary XML Files") { Patterns = new[] { "*.xml" } } }
        });

        if (files.Count > 0)
        {
            string xmlPath = files[0].Path.LocalPath;
            string txtPath = Path.ChangeExtension(xmlPath, ".txt.xml"); // Use .txt.xml to distinguish from binary

            try
            {
                Log($"Extracting {Path.GetFileName(xmlPath)} to Readable XML...");
                await XmlConverter.ExtractBinaryXmlToRealXmlAsync(xmlPath, txtPath, Log);
            }
            catch (Exception ex)
            {
                Log($"Error extracting XML: {ex.Message}");
            }
        }
    }

    private async void BtnRepackXml_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var txtFiles = await topLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select edited readable .xml file",
            AllowMultiple = false,
            FileTypeFilter = new[] { new FilePickerFileType("Readable XML Files") { Patterns = new[] { "*.xml" } } }
        });

        if (txtFiles.Count == 0) return;
        string txtPath = txtFiles[0].Path.LocalPath;

        var xmlFiles = await topLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select original binary .xml file to inject into",
            AllowMultiple = false,
            FileTypeFilter = new[] { new FilePickerFileType("Binary XML Files") { Patterns = new[] { "*.xml" } } }
        });

        if (xmlFiles.Count == 0) return;
        string originalXmlPath = xmlFiles[0].Path.LocalPath;

        var saveFile = await topLevel.StorageProvider.SaveFilePickerAsync(new FilePickerSaveOptions
        {
            Title = "Save repacked binary .xml file",
            DefaultExtension = "xml",
            SuggestedFileName = Path.GetFileName(originalXmlPath),
            FileTypeChoices = new[] { new FilePickerFileType("Binary XML Files") { Patterns = new[] { "*.xml" } } }
        });

        if (saveFile != null)
        {
            string outXmlPath = saveFile.Path.LocalPath;
            try
            {
                Log($"Repacking {Path.GetFileName(txtPath)} into {Path.GetFileName(outXmlPath)}...");
                await XmlConverter.RepackRealXmlToBinaryXmlAsync(originalXmlPath, txtPath, outXmlPath, Log);
            }
            catch (Exception ex)
            {
                Log($"Error repacking XML: {ex.Message}");
            }
        }
    }

    private async void BtnBatchExtractXml_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select folder containing Gamebryo binary .xml files",
            AllowMultiple = false
        });

        if (folders.Count > 0)
        {
            string folderPath = folders[0].Path.LocalPath;
            string[] xmlFiles = Directory.GetFiles(folderPath, "*.xml", SearchOption.AllDirectories);

            if (xmlFiles.Length == 0)
            {
                Log("No .xml files found in the selected folder or its subfolders.");
                return;
            }

            Log($"Found {xmlFiles.Length} XML files. Starting batch extraction...");
            int successCount = 0;

            foreach (var xmlPath in xmlFiles)
            {
                if (xmlPath.EndsWith(".txt.xml", StringComparison.OrdinalIgnoreCase)) continue; // Skip already extracted ones
                string txtPath = Path.ChangeExtension(xmlPath, ".txt.xml");
                bool success = await XmlConverter.ExtractBinaryXmlToRealXmlAsync(xmlPath, txtPath, Log);
                if (success) successCount++;
            }

            Log($"Batch extraction complete! Successfully extracted {successCount} files.");
        }
    }

    private async void BtnBatchRepackXml_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select folder containing edited readable .txt.xml AND original binary .xml files",
            AllowMultiple = false
        });

        if (folders.Count > 0)
        {
            string folderPath = folders[0].Path.LocalPath;
            string[] txtFiles = Directory.GetFiles(folderPath, "*.txt.xml", SearchOption.AllDirectories);

            if (txtFiles.Length == 0)
            {
                Log("No .txt.xml files found in the selected folder or its subfolders.");
                return;
            }

            string outDir = Path.Combine(folderPath, "Repacked_XML");
            Directory.CreateDirectory(outDir);

            Log($"Found {txtFiles.Length} readable XML files. Starting batch repack...");
            int successCount = 0;

            foreach (var txtPath in txtFiles)
            {
                // Skip files that are already inside the Repacked_XML folder
                if (txtPath.StartsWith(outDir, StringComparison.OrdinalIgnoreCase)) continue;

                string relativePath = Path.GetRelativePath(folderPath, txtPath);
                string relativeDir = Path.GetDirectoryName(relativePath) ?? "";

                string fileName = Path.GetFileNameWithoutExtension(Path.GetFileNameWithoutExtension(txtPath)); // Remove .txt.xml
                string originalXmlPath = Path.Combine(Path.GetDirectoryName(txtPath)!, fileName + ".xml");

                string targetDir = Path.Combine(outDir, relativeDir);
                Directory.CreateDirectory(targetDir);
                string outXmlPath = Path.Combine(targetDir, fileName + ".xml");

                if (!File.Exists(originalXmlPath))
                {
                    Log($"Skipping {fileName}.txt.xml: Original {fileName}.xml not found in the same folder.");
                    continue;
                }

                bool success = await XmlConverter.RepackRealXmlToBinaryXmlAsync(originalXmlPath, txtPath, outXmlPath, Log);
                if (success) successCount++;
            }

            Log($"Batch repack complete! Successfully repacked {successCount} files into 'Repacked_XML' folder.");
        }
    }

    private void Log(string message)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            var txtLog = this.FindControl<TextBlock>("TxtLog");
            if (txtLog != null) txtLog.Text = message;
        });
    }

    private async void BtnOpenDv2_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var files = await topLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select .dv2 file to open",
            AllowMultiple = false,
            FileTypeFilter = new[] { new FilePickerFileType("Divinity 2 Archive") { Patterns = new[] { "*.dv2" } } }
        });

        if (files.Count > 0)
        {
            _currentDv2Path = files[0].Path.LocalPath;
            _currentFolderPath = null;

            var btnUnpack = this.FindControl<Button>("BtnUnpack");
            var btnPack = this.FindControl<Button>("BtnPack");
            if (btnUnpack != null) btnUnpack.IsEnabled = true;
            if (btnPack != null) btnPack.IsEnabled = false;

            try
            {
                var entries = await Dv2Archive.ReadEntriesAsync(_currentDv2Path);
                BuildTreeFromEntries(entries);
                Log($"Opened {_currentDv2Path}. Found {entries.Count} files.");
            }
            catch (Exception ex)
            {
                Log($"Error opening DV2: {ex.Message}");
            }
        }
    }

    private async void BtnUnpack_Click(object? sender, RoutedEventArgs e)
    {
        if (string.IsNullOrEmpty(_currentDv2Path)) return;

        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select destination folder"
        });

        if (folders.Count > 0)
        {
            string outDir = folders[0].Path.LocalPath;

            var btnUnpack = this.FindControl<Button>("BtnUnpack");
            var btnOpenDv2 = this.FindControl<Button>("BtnOpenDv2");

            if (btnUnpack != null) btnUnpack.IsEnabled = false;
            if (btnOpenDv2 != null) btnOpenDv2.IsEnabled = false;

            try
            {
                await Dv2Archive.UnpackAsync(_currentDv2Path, outDir, Log);
            }
            catch (Exception ex)
            {
                Log($"Error: {ex.Message}");
            }
            finally
            {
                if (btnUnpack != null) btnUnpack.IsEnabled = true;
                if (btnOpenDv2 != null) btnOpenDv2.IsEnabled = true;
            }
        }
    }

    private async void BtnBatchUnpack_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select Root Folder for Batch Unpack",
            AllowMultiple = false
        });

        if (folders.Count == 0) return;

        string rootDir = folders[0].Path.LocalPath;
        string[] dv2Files = Directory.GetFiles(rootDir, "*.dv2", SearchOption.AllDirectories);

        Log($"Found {dv2Files.Length} archives. Starting batch unpack...");

        foreach (var dv2Path in dv2Files)
        {
            string outDir = Path.Combine(Path.GetDirectoryName(dv2Path)!, Path.GetFileNameWithoutExtension(dv2Path) + "_extracted");
            Log($"Unpacking: {Path.GetFileName(dv2Path)}...");
            try
            {
                await Dv2Archive.UnpackAsync(dv2Path, outDir, (msg) => { });
            }
            catch (Exception ex)
            {
                Log($"Error unpacking {dv2Path}: {ex.Message}");
            }
        }
        Log("Batch unpack finished.");
    }

    private async void BtnOpenFolder_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select folder to pack"
        });

        if (folders.Count > 0)
        {
            _currentFolderPath = folders[0].Path.LocalPath;
            _currentDv2Path = null;

            var btnUnpack = this.FindControl<Button>("BtnUnpack");
            var btnPack = this.FindControl<Button>("BtnPack");
            if (btnUnpack != null) btnUnpack.IsEnabled = false;
            if (btnPack != null) btnPack.IsEnabled = true;

            try
            {
                BuildTreeFromDirectory(_currentFolderPath);
                Log($"Opened folder {_currentFolderPath}. Ready to pack.");
            }
            catch (Exception ex)
            {
                Log($"Error reading folder: {ex.Message}");
            }
        }
    }

    private async void BtnPack_Click(object? sender, RoutedEventArgs e)
    {
        if (string.IsNullOrEmpty(_currentFolderPath)) return;

        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var file = await topLevel.StorageProvider.SaveFilePickerAsync(new FilePickerSaveOptions
        {
            Title = "Save .dv2 archive",
            DefaultExtension = "dv2",
            FileTypeChoices = new[] { new FilePickerFileType("Divinity 2 Archive") { Patterns = new[] { "*.dv2" } } }
        });

        if (file != null)
        {
            string outPath = file.Path.LocalPath;

            var chkCompress = this.FindControl<CheckBox>("ChkCompress");
            bool compress = chkCompress?.IsChecked ?? true;

            var cmbCompression = this.FindControl<ComboBox>("CmbCompressionLevel");
            CompressionLevel compLevel = CompressionLevel.Optimal;
            if (cmbCompression != null)
            {
                compLevel = cmbCompression.SelectedIndex switch
                {
                    1 => CompressionLevel.SmallestSize,
                    2 => CompressionLevel.Fastest,
                    3 => CompressionLevel.NoCompression,
                    _ => CompressionLevel.Optimal
                };
            }

            var btnPack = this.FindControl<Button>("BtnPack");
            var btnOpenFolder = this.FindControl<Button>("BtnOpenFolder");

            if (btnPack != null) btnPack.IsEnabled = false;
            if (btnOpenFolder != null) btnOpenFolder.IsEnabled = false;

            try
            {
                await Dv2Archive.PackAsync(_currentFolderPath, outPath, compress, compLevel, Log);
            }
            catch (Exception ex)
            {
                Log($"Error: {ex.Message}");
            }
            finally
            {
                if (btnPack != null) btnPack.IsEnabled = true;
                if (btnOpenFolder != null) btnOpenFolder.IsEnabled = true;
            }
        }
    }

    private async void BtnConvertDds_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var files = await topLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select .dds file to convert",
            AllowMultiple = false,
            FileTypeFilter = new[] { new FilePickerFileType("DirectDraw Surface") { Patterns = new[] { "*.dds" } } }
        });

        if (files.Count > 0)
        {
            string ddsPath = files[0].Path.LocalPath;
            string nifPath = Path.ChangeExtension(ddsPath, ".nif");

            try
            {
                Log($"Converting {Path.GetFileName(ddsPath)} to NIF...");
                await DdsToNifConverter.ConvertAsync(ddsPath, nifPath, Log);
            }
            catch (Exception ex)
            {
                Log($"Error converting DDS: {ex.Message}");
            }
        }
    }

    private async void BtnExtractNif_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var files = await topLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Select .nif file to extract",
            AllowMultiple = false,
            FileTypeFilter = new[] { new FilePickerFileType("Gamebryo NIF") { Patterns = new[] { "*.nif" } } }
        });

        if (files.Count > 0)
        {
            string nifPath = files[0].Path.LocalPath;
            string ddsPath = Path.ChangeExtension(nifPath, ".dds");

            try
            {
                Log($"Extracting {Path.GetFileName(nifPath)} to DDS...");
                bool success = await DdsToNifConverter.ExtractNifToDdsAsync(nifPath, ddsPath, Log);
                if (!success)
                {
                    Log($"File {Path.GetFileName(nifPath)} does not contain texture data.");
                }
            }
            catch (Exception ex)
            {
                Log($"Error extracting NIF: {ex.Message}");
            }
        }
    }

    private async void BtnBatchDdsToNif_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select folder containing .dds files",
            AllowMultiple = false
        });

        if (folders.Count > 0)
        {
            string folderPath = folders[0].Path.LocalPath;
            Log($"Starting batch conversion of DDS to NIF in {folderPath}...");
            await DdsToNifConverter.BatchConvertDdsToNifAsync(folderPath, Log);
        }
    }

    private async void BtnBatchNifToDds_Click(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel == null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Select folder containing .nif files",
            AllowMultiple = false
        });

        if (folders.Count > 0)
        {
            string folderPath = folders[0].Path.LocalPath;
            Log($"Starting batch extraction of NIF to DDS in {folderPath}...");
            await DdsToNifConverter.BatchExtractNifToDdsAsync(folderPath, Log);
        }
    }

    private static readonly char[] PathSeparators = ['\\', '/'];

    private void BuildTreeFromEntries(List<Dv2Entry> entries)
    {
        _treeItems.Clear();
        var rootNode = new FileTreeNode { Name = Path.GetFileName(_currentDv2Path) ?? "Archive", IsDirectory = true };
        _treeItems.Add(rootNode);

        var dict = new Dictionary<string, FileTreeNode>(StringComparer.OrdinalIgnoreCase);
        dict[""] = rootNode;

        foreach (var entry in entries)
        {
            string[] parts = entry.Name.Split(PathSeparators, StringSplitOptions.RemoveEmptyEntries);
            string currentPath = "";
            FileTreeNode parent = rootNode;

            for (int i = 0; i < parts.Length; i++)
            {
                string part = parts[i];
                string newPath = string.IsNullOrEmpty(currentPath) ? part : currentPath + "\\" + part;

                if (!dict.TryGetValue(newPath, out var node))
                {
                    node = new FileTreeNode
                    {
                        Name = part,
                        FullPath = newPath,
                        IsDirectory = i < parts.Length - 1
                    };
                    parent.Children.Add(node);
                    dict[newPath] = node;
                }
                parent = node;
                currentPath = newPath;
            }
        }
    }

    private void BuildTreeFromDirectory(string dirPath)
    {
        _treeItems.Clear();
        var rootNode = new FileTreeNode { Name = Path.GetFileName(dirPath) ?? "Folder", IsDirectory = true, FullPath = dirPath };
        _treeItems.Add(rootNode);

        PopulateDirectoryNode(rootNode, dirPath);
    }

    private void PopulateDirectoryNode(FileTreeNode parentNode, string dirPath)
    {
        try
        {
            foreach (var dir in Directory.GetDirectories(dirPath))
            {
                var node = new FileTreeNode
                {
                    Name = Path.GetFileName(dir),
                    FullPath = dir,
                    IsDirectory = true
                };
                parentNode.Children.Add(node);
                PopulateDirectoryNode(node, dir);
            }

            foreach (var file in Directory.GetFiles(dirPath))
            {
                var node = new FileTreeNode
                {
                    Name = Path.GetFileName(file),
                    FullPath = file,
                    IsDirectory = false
                };
                parentNode.Children.Add(node);
            }
        }
        catch (Exception ex)
        {
            Log($"Error reading directory {dirPath}: {ex.Message}");
        }
    }
}
