using Avalonia;
using System;

namespace D2DCTool;

class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        if (System.IO.File.Exists("hash_dictionary.txt"))
        {
            XmlConverter.LoadHashes(System.IO.File.ReadAllLines("hash_dictionary.txt"));
        }

        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
}
