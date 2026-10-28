using System.Collections.Concurrent;
using UnityEngine;

namespace Unity.Profiling
{
    /// <summary>
    /// A profiler category: a name, a color and a 16-bit id. Ids count up from 0 in the order names are first
    /// used in this process and are the same for every category of one name. The static categories use Unity's
    /// display names ("GUI", "AI", "VR", "Virtual Texturing", "File IO", and the plain name for the rest);
    /// their palette colors are the engine's own approximation of Unity's.
    /// </summary>
    public readonly struct ProfilerCategory
    {
        private static readonly ConcurrentDictionary<string, ushort> Ids = new(StringComparer.Ordinal);
        private static int _lastId = -1;

        private readonly string? _name;
        private readonly ushort _id;
        private readonly ProfilerCategoryColor _color;

        public ProfilerCategory(string categoryName) : this(categoryName, ProfilerCategoryColor.Scripts) { }

        public ProfilerCategory(string categoryName, ProfilerCategoryColor color)
        {
            ArgumentNullException.ThrowIfNull(categoryName);
            _name = categoryName;
            _color = color;
            _id = Ids.GetOrAdd(categoryName, static _ => (ushort)Interlocked.Increment(ref _lastId));
        }

        public string Name => _name ?? "";

        public Color32 Color => _color switch
        {
            ProfilerCategoryColor.Render => new Color32(0x4d, 0x8f, 0xd6, 0xff),
            ProfilerCategoryColor.Scripts => new Color32(0x4a, 0xb6, 0xe8, 0xff),
            ProfilerCategoryColor.Physics => new Color32(0xe0, 0x8a, 0x2c, 0xff),
            ProfilerCategoryColor.Animation => new Color32(0x6d, 0xc0, 0x6a, 0xff),
            ProfilerCategoryColor.Audio => new Color32(0xb0, 0x6a, 0xd0, 0xff),
            ProfilerCategoryColor.Lighting => new Color32(0xf0, 0xd0, 0x40, 0xff),
            ProfilerCategoryColor.Memory => new Color32(0xc0, 0x50, 0x50, 0xff),
            ProfilerCategoryColor.Internal => new Color32(0x80, 0x80, 0x80, 0xff),
            ProfilerCategoryColor.UI => new Color32(0x60, 0xc0, 0xb0, 0xff),
            ProfilerCategoryColor.Input => new Color32(0xd0, 0x70, 0x90, 0xff),
            _ => new Color32(0xa0, 0xa0, 0xa0, 0xff),
        };

        public override string ToString() => Name;

        public static implicit operator ushort(ProfilerCategory category) => category._id;

        public static ProfilerCategory Render => new("Render", ProfilerCategoryColor.Render);
        public static ProfilerCategory Scripts => new("Scripts", ProfilerCategoryColor.Scripts);
        public static ProfilerCategory Gui => new("GUI", ProfilerCategoryColor.UI);
        public static ProfilerCategory Physics => new("Physics", ProfilerCategoryColor.Physics);
        public static ProfilerCategory Physics2D => new("Physics2D", ProfilerCategoryColor.Physics);
        public static ProfilerCategory Animation => new("Animation", ProfilerCategoryColor.Animation);
        public static ProfilerCategory Ai => new("AI", ProfilerCategoryColor.Other);
        public static ProfilerCategory Audio => new("Audio", ProfilerCategoryColor.Audio);
        public static ProfilerCategory Video => new("Video", ProfilerCategoryColor.Other);
        public static ProfilerCategory Particles => new("Particles", ProfilerCategoryColor.Other);
        public static ProfilerCategory Lighting => new("Lighting", ProfilerCategoryColor.Lighting);
        public static ProfilerCategory Network => new("Network", ProfilerCategoryColor.Other);
        public static ProfilerCategory Loading => new("Loading", ProfilerCategoryColor.Other);
        public static ProfilerCategory Vr => new("VR", ProfilerCategoryColor.Other);
        public static ProfilerCategory Input => new("Input", ProfilerCategoryColor.Input);
        public static ProfilerCategory Memory => new("Memory", ProfilerCategoryColor.Memory);
        public static ProfilerCategory VirtualTexturing => new("Virtual Texturing", ProfilerCategoryColor.Other);
        public static ProfilerCategory FileIO => new("File IO", ProfilerCategoryColor.Other);
        public static ProfilerCategory Internal => new("Internal", ProfilerCategoryColor.Internal);
    }
}
