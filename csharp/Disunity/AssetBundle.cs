namespace UnityEngine;

/// <summary>
/// A file of packed assets. The type exists so code that references it loads. The engine does not
/// load asset bundles yet, so <see cref="LoadFromFile"/> and every instance member throw
/// <see cref="NotSupportedException"/> naming the member.
/// </summary>
public sealed class AssetBundle : Object
{
    internal AssetBundle() { }

    public static AssetBundle LoadFromFile(string path) => throw Unsupported(nameof(LoadFromFile));

    public T LoadAsset<T>(string name) where T : Object => throw Unsupported(nameof(LoadAsset));

    public Object LoadAsset(string name) => throw Unsupported(nameof(LoadAsset));

    public T[] LoadAssetWithSubAssets<T>(string name) where T : Object => throw Unsupported(nameof(LoadAssetWithSubAssets));

    public T[] LoadAllAssets<T>() where T : Object => throw Unsupported(nameof(LoadAllAssets));

    public string[] GetAllAssetNames() => throw Unsupported(nameof(GetAllAssetNames));

    public void Unload(bool unloadAllLoadedObjects) => throw Unsupported(nameof(Unload));

    private static NotSupportedException Unsupported(string member) =>
        new($"AssetBundle.{member}: asset bundles are not loaded yet");
}
