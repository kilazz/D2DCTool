using System.Collections.ObjectModel;

namespace D2DCTool;

public class FileTreeNode
{
    public string Name { get; set; } = string.Empty;
    public string FullPath { get; set; } = string.Empty;
    public bool IsDirectory { get; set; }
    public ObservableCollection<FileTreeNode> Children { get; } = [];
}
