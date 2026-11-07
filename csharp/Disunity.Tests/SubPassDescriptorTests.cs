using UnityEngine.Rendering;

/// <summary>AttachmentIndexArray and SubPassDescriptor hold their data and compare by value.</summary>
internal static class SubPassDescriptorTests
{
    public static void Run()
    {
        var empty = default(AttachmentIndexArray);
        Expect.Equal(0, empty.Length, "the default list is empty");
        var list = new AttachmentIndexArray(3);
        Expect.Equal(3, list.Length, "a new list has its length");
        for (var i = 0; i < 3; i++) Expect.Equal(-1, list[i], $"index {i} starts unset");
        list[0] = 5;
        list[2] = 7;
        Expect.That(list[0] == 5 && list[1] == -1 && list[2] == 7, "indices keep what was set");
        var full = new AttachmentIndexArray(AttachmentIndexArray.MaxAttachments);
        for (var i = 0; i < 8; i++) full[i] = 10 + i;
        Expect.Sequence(Enumerable.Range(10, 8), Enumerable.Range(0, 8).Select(i => full[i]), "all eight slots are independent");
        GraphicsTestSupport.Throws<IndexOutOfRangeException>(() => _ = list[3], "an index past the length");
        GraphicsTestSupport.Throws<IndexOutOfRangeException>(() => list[-1] = 1, "a negative index");
        GraphicsTestSupport.Throws<IndexOutOfRangeException>(() => _ = empty[0], "an index into the default list");
        GraphicsTestSupport.Throws<ArgumentOutOfRangeException>(() => new AttachmentIndexArray(9), "a list over the maximum");
        GraphicsTestSupport.Throws<ArgumentOutOfRangeException>(() => new AttachmentIndexArray(-1), "a negative length");
        var same = new AttachmentIndexArray(3);
        same[0] = 5;
        same[2] = 7;
        Expect.That(list == same && list.GetHashCode() == same.GetHashCode() && list.Equals((object)same), "lists with the same length and indices are equal");
        same[1] = 0;
        Expect.That(list != same, "a different index makes them differ");
        Expect.That(new AttachmentIndexArray(2) != new AttachmentIndexArray(3), "a different length makes them differ");

        var sub = new SubPassDescriptor();
        Expect.That(sub.inputs.Length == 0 && sub.colorOutputs.Length == 0 && sub.flags == SubPassFlags.None, "a new sub-pass is empty");
        sub.inputs = list;
        sub.colorOutputs = same;
        sub.flags = SubPassFlags.ReadOnlyDepth;
        var copy = new SubPassDescriptor { inputs = list, colorOutputs = same, flags = SubPassFlags.ReadOnlyDepth };
        Expect.That(sub == copy && sub.GetHashCode() == copy.GetHashCode() && sub.Equals((object)copy), "sub-passes with the same data are equal");
        copy.flags = SubPassFlags.None;
        Expect.That(sub != copy, "a different flag makes sub-passes differ");
        Console.WriteLine("PASS: AttachmentIndexArray and SubPassDescriptor hold their indices and flags and compare by value");
    }
}
