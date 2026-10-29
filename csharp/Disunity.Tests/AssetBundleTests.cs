using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;
using Object = UnityEngine.Object;

/// <summary>AssetBundle exists so code that references it loads; every member says that bundles are not loaded yet.</summary>
internal static class AssetBundleTests
{
    public static void Run()
    {
        Ensure(typeof(Object).IsAssignableFrom(typeof(AssetBundle)), "AssetBundle is an Object");
        Ensure(ModuleFacades.Forwards(typeof(DisunityHost).Assembly, "UnityEngine.AssetBundle"), "the module facades forward AssetBundle");
        Throws<NotSupportedException>(() => AssetBundle.LoadFromFile("mods/a.bundle"), "LoadFromFile", "AssetBundle.LoadFromFile", "not loaded yet");
        var bundle = (AssetBundle)Activator.CreateInstance(typeof(AssetBundle), nonPublic: true)!;
        Throws<NotSupportedException>(() => bundle.LoadAsset<Object>("a"), "LoadAsset<T>", "AssetBundle.LoadAsset", "not loaded yet");
        Throws<NotSupportedException>(() => bundle.LoadAsset("a"), "LoadAsset", "AssetBundle.LoadAsset", "not loaded yet");
        Throws<NotSupportedException>(() => bundle.LoadAssetWithSubAssets<Object>("a"), "LoadAssetWithSubAssets", "AssetBundle.LoadAssetWithSubAssets", "not loaded yet");
        Throws<NotSupportedException>(() => bundle.LoadAllAssets<Object>(), "LoadAllAssets", "AssetBundle.LoadAllAssets", "not loaded yet");
        Throws<NotSupportedException>(() => bundle.GetAllAssetNames(), "GetAllAssetNames", "AssetBundle.GetAllAssetNames", "not loaded yet");
        Throws<NotSupportedException>(() => bundle.Unload(true), "Unload", "AssetBundle.Unload", "not loaded yet");
        Console.WriteLine("PASS: AssetBundle exists and every member throws NotSupportedException naming itself");
    }
}
