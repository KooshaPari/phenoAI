using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace PlayCua;

/// <summary>
/// Thin C# client for the playcua-native binary via stdio JSON-RPC 2.0.
/// Drop-in replacement for DINOForge MCP server screenshot/input tools.
/// </summary>
/// <remarks>
/// Usage:
/// <code>
/// await using var computer = await NativeComputer.StartAsync();
/// byte[] png = await computer.ScreenshotAsync();
/// await computer.ClickAsync(100, 200);
/// await computer.TypeTextAsync("hello world");
/// </code>
/// </remarks>
public sealed class NativeComputer : IAsyncDisposable
{
    private Process? _proc;
    private int _id;
    private readonly SemaphoreSlim _lock = new(1, 1);

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private NativeComputer() { }

    /// <summary>
    /// Start the native binary and verify it responds to ping.
    /// </summary>
    /// <param name="nativePath">Path or name of the playcua-native executable.</param>
    /// <param name="logLevel">Value for PLAYCUA_LOG env var (Rust tracing level).</param>
    /// <param name="ct">Cancellation token.</param>
    public static async Task<NativeComputer> StartAsync(
        string nativePath = "playcua-native",
        string logLevel = "info",
        CancellationToken ct = default)
    {
        var psi = new ProcessStartInfo(nativePath)
        {
            UseShellExecute = false,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = false, // let stderr flow to console for logging
            StandardInputEncoding = Encoding.UTF8,
            StandardOutputEncoding = Encoding.UTF8,
        };
        psi.Environment["PLAYCUA_LOG"] = logLevel;

        var computer = new NativeComputer
        {
            _proc = Process.Start(psi) ?? throw new InvalidOperationException($"Failed to start {nativePath}")
        };

        // Verify alive.
        var pong = await computer.CallAsync("ping", null, ct);
        if (pong.ValueKind == JsonValueKind.Undefined || !pong.TryGetProperty("ok", out _))
            throw new InvalidOperationException("playcua-native did not respond to ping");

        return computer;
    }

    // -----------------------------------------------------------------------
    // Screenshot
    // -----------------------------------------------------------------------

    /// <summary>Capture a screenshot. Returns raw PNG bytes.</summary>
    /// <param name="windowTitle">Optional partial window title to capture.</param>
    /// <param name="monitor">Zero-based monitor index (used when windowTitle is null).</param>
    public async Task<byte[]> ScreenshotAsync(
        string? windowTitle = null,
        int monitor = 0,
        CancellationToken ct = default)
    {
        var result = await CallAsync("screenshot", new
        {
            window_title = windowTitle,
            monitor,
        }, ct);

        var b64 = result.GetProperty("data").GetString()
                  ?? throw new InvalidOperationException("screenshot: missing data field");
        return Convert.FromBase64String(b64);
    }

    // -----------------------------------------------------------------------
    // Mouse
    // -----------------------------------------------------------------------

    /// <summary>Click a mouse button at the given coordinates.</summary>
    public async Task ClickAsync(
        int x,
        int y,
        string button = "left",
        string action = "click",
        CancellationToken ct = default)
    {
        await CallAsync("input.click", new { x, y, button, action }, ct);
    }

    /// <summary>Double-click the left button at the given coordinates.</summary>
    public async Task DoubleClickAsync(int x, int y, CancellationToken ct = default)
    {
        await ClickAsync(x, y, "left", "click", ct);
        await Task.Delay(50, ct);
        await ClickAsync(x, y, "left", "click", ct);
    }

    /// <summary>Scroll at coordinates.</summary>
    public async Task ScrollAsync(
        int x,
        int y,
        string direction = "down",
        int amount = 3,
        CancellationToken ct = default)
    {
        await CallAsync("input.scroll", new { x, y, direction, amount }, ct);
    }

    /// <summary>Move the mouse cursor.</summary>
    public async Task MoveMouseAsync(int x, int y, CancellationToken ct = default)
    {
        await CallAsync("input.move", new { x, y }, ct);
    }

    // -----------------------------------------------------------------------
    // Keyboard
    // -----------------------------------------------------------------------

    /// <summary>Type a string of text.</summary>
    public async Task TypeTextAsync(string text, CancellationToken ct = default)
    {
        await CallAsync("input.type", new { text }, ct);
    }

    /// <summary>Press (down + up) a named key.</summary>
    public async Task PressKeyAsync(string key, CancellationToken ct = default)
    {
        await CallAsync("input.key", new { key, action = "press" }, ct);
    }

    /// <summary>Hold a key down.</summary>
    public async Task KeyDownAsync(string key, CancellationToken ct = default)
    {
        await CallAsync("input.key", new { key, action = "down" }, ct);
    }

    /// <summary>Release a held key.</summary>
    public async Task KeyUpAsync(string key, CancellationToken ct = default)
    {
        await CallAsync("input.key", new { key, action = "up" }, ct);
    }

    // -----------------------------------------------------------------------
    // Windows
    // -----------------------------------------------------------------------

    /// <summary>List all top-level windows.</summary>
    public async Task<IReadOnlyList<WindowInfo>> ListWindowsAsync(CancellationToken ct = default)
    {
        var result = await CallAsync("windows.list", new { }, ct);
        var list = JsonSerializer.Deserialize<List<WindowInfo>>(result.GetRawText(), JsonOptions)
                   ?? [];
        return list;
    }

    /// <summary>Find a window by partial title or PID. Returns null if not found.</summary>
    public async Task<WindowInfo?> FindWindowAsync(
        string? title = null,
        int? pid = null,
        CancellationToken ct = default)
    {
        var result = await CallAsync("windows.find", new { title, pid }, ct);
        if (result.ValueKind == JsonValueKind.Null)
            return null;
        return JsonSerializer.Deserialize<WindowInfo>(result.GetRawText(), JsonOptions);
    }

    /// <summary>Bring a window to the foreground by HWND.</summary>
    public async Task FocusWindowAsync(long hwnd, CancellationToken ct = default)
    {
        await CallAsync("windows.focus", new { hwnd }, ct);
    }

    // -----------------------------------------------------------------------
    // Process
    // -----------------------------------------------------------------------

    /// <summary>Launch a process non-blocking. Returns the PID.</summary>
    public async Task<int> LaunchProcessAsync(
        string path,
        string[]? args = null,
        string? cwd = null,
        CancellationToken ct = default)
    {
        var result = await CallAsync("process.launch", new { path, args, cwd }, ct);
        return result.GetProperty("pid").GetInt32();
    }

    /// <summary>Kill a process by PID.</summary>
    public async Task KillProcessAsync(int pid, CancellationToken ct = default)
    {
        await CallAsync("process.kill", new { pid }, ct);
    }

    /// <summary>Check whether a process is still running.</summary>
    public async Task<ProcessStatus> ProcessStatusAsync(int pid, CancellationToken ct = default)
    {
        var result = await CallAsync("process.status", new { pid }, ct);
        return JsonSerializer.Deserialize<ProcessStatus>(result.GetRawText(), JsonOptions)
               ?? new ProcessStatus(false, null);
    }

    // -----------------------------------------------------------------------
    // Analysis
    // -----------------------------------------------------------------------

    /// <summary>Return true if two PNG images differ by more than threshold fraction of pixels.</summary>
    public async Task<bool> FramesDifferAsync(
        byte[] imageA,
        byte[] imageB,
        double threshold = 0.02,
        CancellationToken ct = default)
    {
        var result = await CallAsync("analysis.diff", new
        {
            image_a = Convert.ToBase64String(imageA),
            image_b = Convert.ToBase64String(imageB),
            threshold,
        }, ct);
        return result.GetProperty("changed").GetBoolean();
    }

    /// <summary>Return a BLAKE3 hex hash of the image pixel data.</summary>
    public async Task<string> ImageHashAsync(byte[] image, CancellationToken ct = default)
    {
        var result = await CallAsync("analysis.hash", new
        {
            image = Convert.ToBase64String(image),
        }, ct);
        return result.GetProperty("hash").GetString() ?? string.Empty;
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    /// <summary>Verify the native binary is alive. Returns true on success.</summary>
    public async Task<bool> PingAsync(CancellationToken ct = default)
    {
        try
        {
            var result = await CallAsync("ping", new { }, ct);
            return result.TryGetProperty("ok", out var ok) && ok.GetBoolean();
        }
        catch
        {
            return false;
        }
    }

    // -----------------------------------------------------------------------
    // Low-level JSON-RPC transport
    // -----------------------------------------------------------------------

    private async Task<JsonElement> CallAsync(
        string method,
        object? @params,
        CancellationToken ct = default)
    {
        if (_proc is null || _proc.HasExited)
            throw new ObjectDisposedException(nameof(NativeComputer), "Native process is not running");

        await _lock.WaitAsync(ct);
        try
        {
            int id = Interlocked.Increment(ref _id);
            var request = new
            {
                jsonrpc = "2.0",
                id,
                method,
                @params = @params ?? new { },
            };

            string reqJson = JsonSerializer.Serialize(request, JsonOptions) + "\n";
            await _proc.StandardInput.WriteAsync(reqJson.AsMemory(), ct);
            await _proc.StandardInput.FlushAsync(ct);

            string? respLine = await _proc.StandardOutput.ReadLineAsync(ct);
            if (respLine is null)
                throw new InvalidOperationException("playcua-native closed stdout unexpectedly");

            using JsonDocument doc = JsonDocument.Parse(respLine);
            var root = doc.RootElement.Clone();

            if (root.TryGetProperty("error", out JsonElement errEl) &&
                errEl.ValueKind != JsonValueKind.Null)
            {
                int code = errEl.TryGetProperty("code", out var c) ? c.GetInt32() : -1;
                string msg = errEl.TryGetProperty("message", out var m)
                    ? m.GetString() ?? "unknown"
                    : "unknown";
                throw new RpcException(code, msg);
            }

            if (!root.TryGetProperty("result", out JsonElement resultEl))
                return default;

            return resultEl;
        }
        finally
        {
            _lock.Release();
        }
    }

    // -----------------------------------------------------------------------
    // IAsyncDisposable
    // -----------------------------------------------------------------------

    public async ValueTask DisposeAsync()
    {
        if (_proc is not null)
        {
            try
            {
                _proc.StandardInput.Close();
                await _proc.WaitForExitAsync(CancellationToken.None)
                    .WaitAsync(TimeSpan.FromSeconds(3));
            }
            catch
            {
                try { _proc.Kill(); } catch { /* ignore */ }
            }
            _proc.Dispose();
            _proc = null;
        }
        _lock.Dispose();
    }
}

// -----------------------------------------------------------------------
// Supporting types
// -----------------------------------------------------------------------

/// <summary>Metadata about a top-level window.</summary>
public sealed record WindowInfo(
    [property: JsonPropertyName("hwnd")]    long   Hwnd,
    [property: JsonPropertyName("title")]   string Title,
    [property: JsonPropertyName("pid")]     uint   Pid,
    [property: JsonPropertyName("x")]       int    X,
    [property: JsonPropertyName("y")]       int    Y,
    [property: JsonPropertyName("width")]   int    Width,
    [property: JsonPropertyName("height")]  int    Height,
    [property: JsonPropertyName("visible")] bool   Visible
);

/// <summary>Process running state.</summary>
public sealed record ProcessStatus(
    [property: JsonPropertyName("running")]   bool Running,
    [property: JsonPropertyName("exit_code")] int? ExitCode
);

/// <summary>Thrown when the native binary returns a JSON-RPC error object.</summary>
public sealed class RpcException : Exception
{
    public int Code { get; }

    public RpcException(int code, string message) : base($"RPC error {code}: {message}")
    {
        Code = code;
    }
}
