using System;
using System.Security.Cryptography;
using System.Text;

namespace StationeersBridge.Feasibility;

internal static class SourceHash
{
    public static string Sha256(string value)
    {
        using var hash = SHA256.Create();
        var bytes = hash.ComputeHash(Encoding.UTF8.GetBytes(value));
        return BitConverter.ToString(bytes).Replace("-", string.Empty).ToLowerInvariant();
    }
}
