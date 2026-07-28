using System;
using System.Collections.Generic;
using BepInEx.Logging;
using Newtonsoft.Json;

namespace StationeersBridge.Feasibility;

internal static class ProbeLog
{
    private const string Prefix = "[P3.02] ";
    private static readonly ManualLogSource Log =
        BepInEx.Logging.Logger.CreateLogSource("P3.02 Feasibility");

    public static void Write(string probe, string status, object? data = null)
    {
        var record = new Dictionary<string, object?>
        {
            ["timestampUtc"] = DateTime.UtcNow.ToString("O"),
            ["probe"] = probe,
            ["status"] = status,
            ["data"] = data,
        };

        Log.LogInfo(Prefix + JsonConvert.SerializeObject(record, Formatting.None));
    }

    public static void Failure(string probe, Exception exception)
    {
        Write(
            probe,
            "failed",
            new
            {
                exceptionType = exception.GetType().FullName,
                message = exception.Message,
            });
    }
}
