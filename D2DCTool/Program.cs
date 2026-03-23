using Avalonia;
using System;

namespace D2DCTool;

class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        if (System.IO.File.Exists("tags.txt"))
        {
            XmlConverter.LoadHashes(System.IO.File.ReadAllLines("tags.txt"));
        }

        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
}
