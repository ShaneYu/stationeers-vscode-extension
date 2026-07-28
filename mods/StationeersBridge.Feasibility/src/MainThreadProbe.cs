using System;
using System.Threading.Tasks;
using Assets.Scripts.Util;

namespace StationeersBridge.Feasibility;

internal static class MainThreadProbe
{
    public static void Run()
    {
        var entryThread = Environment.CurrentManagedThreadId;
        ProbeLog.Write(
            "main-thread-dispatch",
            "entry",
            new { entryThread, dispatcherExists = UnityMainThreadDispatcher.Exists() });

        _ = Task.Run(
            () =>
            {
                var workerThread = Environment.CurrentManagedThreadId;
                UnityMainThreadDispatcher.Instance().Enqueue(
                    () =>
                    {
                        ProbeLog.Write(
                            "main-thread-dispatch",
                            "returned",
                            new
                            {
                                entryThread,
                                workerThread,
                                returnThread = Environment.CurrentManagedThreadId,
                                returnedToEntryThread =
                                    Environment.CurrentManagedThreadId == entryThread,
                            });
                    });
            });
    }
}
