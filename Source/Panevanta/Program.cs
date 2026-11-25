namespace Veyra;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        var root = Environment.GetEnvironmentVariable("VEYRA_ENGINE_DIR") ?? AppContext.BaseDirectory;
        string? smoke = args.Length > 1 && args[0] == "--smoke" ? args[1] : null;
        using var instance = new Mutex(true, @"Local\Veyra.Desktop.Capture", out bool first);
        if (first) Application.Run(new ControlPanel(root, smoke));
        else MessageBox.Show("Panevanta уже запущена.", "Panevanta");
    }
}



