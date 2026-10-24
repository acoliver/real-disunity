using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using CompatibilityBridge;

internal static class ContextualCleanupCodecBehavior
{
    private const string Context = """{"lease":{"world":1,"session":"s","frame":0,"fixed_tick":0,"phase":"Cleanup","sequence":1,"token":"t"},"root_phase":"Cleanup","phase":"destroy","flags":1,"time":{"numerator":0,"denominator":1},"dynamic_delta":-0.0,"fixed_delta":0.02,"unscaled_delta":0.0,"callback_batch":1}""";
    private static int _checks;

    internal static void Run()
    {
        Accept(Envelope(4096), 4096);
        Accept(Envelope(0), 0);
        Reject(Envelope(4097));
        var valid = Envelope(4096);
        Reject(Last(valid, "\"payload\":{}", "\"payload\":{\"x\":0}"));
        Reject(Last(valid, "\"kind\":\"OnDestroy\"", "\"kind\":\"Update\""));
        Reject(Last(valid, "\"payload\":{}", "\"payload\":{},\"payl\\u006fad\":{}"));
        Reject(Last(valid, "\"binding\":4096", "\"binding\":4096,\"binding\":4096"));
        Reject(valid.Replace("\"version\":1", "\"version\":1,\"ver\\u0073ion\":1", StringComparison.Ordinal));
        Reject(Last(valid, "\"binding\":4096,\"component\":4096", "\"binding\":1,\"component\":1"));
        Accept(EnvelopeWith(Record(1) + "," + Record(1).Replace("OnDestroy", "OnDisable", StringComparison.Ordinal)), 2);
        IntegerControls();
        SchemaControls();
        ByteControls();
        OwnershipControls();
        Console.WriteLine($"PASS contextual cleanup codec {_checks} checks; no native or callback execution");
    }

    private static void IntegerControls()
    {
        var valid = Envelope(1);
        foreach (var integer in new[] { "0", "-1", "1e0", "1.0", "01", "9007199254740992", "18446744073709551616", "\"01\"", "\"+1\"", "\"-1\"", "\"1e0\"", "\"1\"", "\"18446744073709551616\"" })
            Reject(valid.Replace("\"binding\":1", "\"binding\":" + integer, StringComparison.Ordinal));
        foreach (var integer in new[] { "9007199254740991", "\"9007199254740992\"", "\"18446744073709551615\"" })
            Accept(valid.Replace("\"binding\":1", "\"binding\":" + integer, StringComparison.Ordinal), 1);
        Reject(valid.Replace("\"numerator\":0", "\"numerator\":-1", StringComparison.Ordinal));
        Reject(valid.Replace("\"denominator\":1", "\"denominator\":0", StringComparison.Ordinal));
        Accept(valid.Replace("\"numerator\":0", "\"numerator\":\"18446744073709551615\"", StringComparison.Ordinal), 1);
    }

    private static void SchemaControls()
    {
        var valid = Envelope(1);
        foreach (var pair in new[] { ("\"flags\":1", "\"flags\":2"), ("\"version\":1", "\"version\":\"1\""), ("\"root_phase\":\"Cleanup\"", "\"root_phase\":\"Update\""), ("\"phase\":\"destroy\"", "\"phase\":\"Cleanup\""), ("\"dynamic_delta\":-0.0", "\"dynamic_delta\":1e100"), ("\"time\":", "\"extra\":0,\"time\":"), ("\"token\":\"t\"", "\"token\":null"), ("\"session\":\"s\"", "\"session\":\"\\ud800\""), ("\"payload\":{}", "\"payload\":[]") })
            Reject(valid.Replace(pair.Item1, pair.Item2, StringComparison.Ordinal));
        Reject(valid.Replace("\"unscaled_delta\":0.0,", "", StringComparison.Ordinal));
        Reject(valid + "{}");
        Reject("{\"callbacks\":[],\"callbacks\":[]}");
        Reject(new string('[', 65) + "0" + new string(']', 65));
        Reject(new string('[', 64) + "0" + new string(']', 64));
        Reject("[" + string.Join(',', Enumerable.Repeat("0", 100001)) + "]");
        Accept(valid.Replace("\"session\":\"s\"", "\"session\":\"s\\ud83d\\ude00\"", StringComparison.Ordinal), 1);
    }

    private static void ByteControls()
    {
        RejectBytes(Array.Empty<byte>());
        RejectBytes(new byte[ContextualCleanupCodec.MaxEnvelopeBytes + 1]);
        var invalid = Encoding.UTF8.GetBytes(Envelope(1));
        invalid[Array.IndexOf(invalid, (byte)'s')] = 0xff;
        RejectBytes(invalid);
        RejectBytes(Encoding.UTF8.GetBytes(Envelope(1)).Concat(new byte[] { 0 }).ToArray());
        var valid = Encoding.UTF8.GetBytes(Envelope(1));
        var boundary = new byte[ContextualCleanupCodec.MaxEnvelopeBytes];
        Array.Fill(boundary, (byte)' ');
        valid.CopyTo(boundary, 0);
        using var batch = ContextualCleanupCodec.Parse(boundary);
        Check(batch.Records.Count == 1 && batch.RawOwnedBytes.Length == boundary.Length, "inclusive byte boundary");
    }

    private static void OwnershipControls()
    {
        var input = Encoding.UTF8.GetBytes(" \n" + Envelope(1) + "\t");
        var original = input.ToArray();
        using var batch = ContextualCleanupCodec.Parse(input);
        input[0] = (byte)'!';
        Check(batch.RawOwnedBytes.Span.SequenceEqual(original), "owned exact original bytes");
        Check(batch.Context.Lease.World == 1 && batch.Context.Time.Numerator == 0 && batch.Context.Time.Denominator == 1, "typed unsigned native context");
        Check(BitConverter.SingleToInt32Bits(batch.Context.DynamicDelta) == int.MinValue, "negative zero f32 preserved");
        Check(BitConverter.SingleToInt32Bits(batch.Context.FixedDelta) == BitConverter.SingleToInt32Bits(0.02f), "native f32 projection");
        Check(batch.LeaseJson.GetProperty("phase").GetString() == "Cleanup", "borrowed document lease");
        var entered = false;
        try { using var rejected = ContextualCleanupCodec.Parse(Encoding.UTF8.GetBytes(Last(Envelope(4096), "\"payload\":{}", "\"payload\":{\"x\":0}"))); entered = true; }
        catch (InvalidDataException) { }
        Check(!entered, "bad last record prevents caller continuation");
    }

    private static void Accept(string json, int count)
    {
        using var batch = ContextualCleanupCodec.Parse(Encoding.UTF8.GetBytes(json));
        Check(batch.Records.Count == count, "accepted callback count");
    }

    private static void Reject(string json) => RejectBytes(Encoding.UTF8.GetBytes(json));

    private static void RejectBytes(byte[] bytes)
    {
        try { using var batch = ContextualCleanupCodec.Parse(bytes); }
        catch (InvalidDataException) { _checks++; return; }
        throw new InvalidOperationException("Malformed envelope accepted");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
        _checks++;
    }

    private static string Record(int index) => "{\"binding\":" + index + ",\"component\":" + index + ",\"kind\":\"OnDestroy\",\"payload\":{}}";
    private static string Envelope(int count) => EnvelopeWith(string.Join(',', Enumerable.Range(1, count).Select(Record)));
    private static string EnvelopeWith(string records) => "{\"version\":1,\"context\":" + Context + ",\"callbacks\":[" + records + "]}";

    private static string Last(string value, string oldValue, string replacement)
    {
        var index = value.LastIndexOf(oldValue, StringComparison.Ordinal);
        if (index < 0) throw new InvalidOperationException("Missing fixture replacement");
        return value[..index] + replacement + value[(index + oldValue.Length)..];
    }
}
