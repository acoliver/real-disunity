using System.Reflection;
using System.Runtime.Loader;
using Disunity.Hosting;

/// <summary>
/// A player's job registration code, generated into each game assembly, instantiates the job package's public
/// generic methods with types the assembly keeps private. Its own runtime allows that; this host's runtime refuses
/// with a <see cref="MethodAccessException"/>. <see cref="PlayerCallbackRewriter"/> gives such an assembly an
/// <c>IgnoresAccessChecksTo</c> attribute that names itself, which makes the runtime skip access checks from the
/// assembly into its own members.
/// </summary>
internal static class JobRegistrationTests
{
    private const string AttributeName = "System.Runtime.CompilerServices.IgnoresAccessChecksToAttribute";

    public static void Run()
    {
        RewritesAJobRegistrationAssemblySoItsPrivateTypeArgumentsAreAccessible();
        LeavesAnAssemblyWithoutJobRegistrationUnchanged();
        RewritingARewrittenImageChangesNothing();
        AppliesBothRewritesToOneImage();
        LoadsTheAccessGrantingImageFromTheCache();
        Console.WriteLine("PASS: job registration assemblies may instantiate generic methods with their own private types (self access rewrite)");
    }

    private static Assembly Load(byte[] image, string name) =>
        new AssemblyLoadContext(name, isCollectible: false).LoadFromStream(new MemoryStream(image));

    private static void Register(Assembly assembly) =>
        assembly.GetType("__JobReflectionRegistrationOutput__1", throwOnError: true)!.GetMethod("Register")!.Invoke(null, null);

    private static void ExpectAccessRefused(Assembly assembly)
    {
        try
        {
            Register(assembly);
        }
        catch (Exception error)
        {
            var cause = error is TargetInvocationException { InnerException: { } inner } ? inner : error;
            Expect.That(cause is MethodAccessException, $"an assembly loaded as it is fails with a MethodAccessException, got {cause}");
            return;
        }
        throw new InvalidOperationException("an assembly loaded as it is must refuse the private type argument, but registration ran");
    }

    private static int SelfGrants(Assembly assembly) => assembly.GetCustomAttributesData()
        .Count(a => a.AttributeType.FullName == AttributeName && a.ConstructorArguments is [{ Value: string name }] && name == assembly.GetName().Name);

    private static void RewritesAJobRegistrationAssemblySoItsPrivateTypeArgumentsAreAccessible()
    {
        var original = JobRegistrationAssembly.Build();
        ExpectAccessRefused(Load(original, "refused"));

        var rewrite = PlayerCallbackRewriter.Rewrite(original);
        Expect.That(rewrite.Image is not null, "an assembly with job registration code is rewritten");
        Expect.That(rewrite.SelfAccess && rewrite.Sites == 0, $"the rewrite reports the access grant and no callback sites, got {rewrite}");
        var assembly = Load(rewrite.Image!, "granted");
        Register(assembly);
        Expect.Equal(1, SelfGrants(assembly), "the assembly names itself once");
    }

    private static void LeavesAnAssemblyWithoutJobRegistrationUnchanged()
    {
        var rewrite = PlayerCallbackRewriter.Rewrite(JobRegistrationAssembly.Build(withRegistration: false));
        Expect.That(rewrite.Image is null && !rewrite.SelfAccess && rewrite.Sites == 0, $"no job registration type, no change, got {rewrite}");
    }

    private static void RewritingARewrittenImageChangesNothing()
    {
        var first = PlayerCallbackRewriter.Rewrite(JobRegistrationAssembly.Build()).Image!;
        var again = PlayerCallbackRewriter.Rewrite(first);
        Expect.That(again.Image is null && !again.SelfAccess && again.Sites == 0, $"a rewritten image is left as it is, got {again}");

        var both = PlayerCallbackRewriter.Rewrite(CallbackAssembly.Build(extend: JobRegistrationAssembly.Define)).Image!;
        var bothAgain = PlayerCallbackRewriter.Rewrite(both);
        Expect.That(bothAgain.Image is null && !bothAgain.SelfAccess && bothAgain.Sites == 0, $"an image with both rewrites is left as it is, got {bothAgain}");
    }

    private static void AppliesBothRewritesToOneImage()
    {
        var rewrite = PlayerCallbackRewriter.Rewrite(CallbackAssembly.Build(extend: JobRegistrationAssembly.Define));
        Expect.That(rewrite.Image is not null && rewrite.Sites == 1 && rewrite.SelfAccess, $"callback sites and the access grant share one image, got {rewrite}");
        var assembly = Load(rewrite.Image!, "both");
        Register(assembly);
        var holder = assembly.GetType("Game.Holder", throwOnError: true)!;
        var make = holder.GetMethod("Make")!;
        Expect.That(ReferenceEquals(make.Invoke(null, null), make.Invoke(null, null)), "the callback delegate is still kept");
        Expect.Equal(1, SelfGrants(assembly), "the combined image names itself once");
    }

    private static void LoadsTheAccessGrantingImageFromTheCache()
    {
        var root = Directory.CreateTempSubdirectory("job-registration");
        try
        {
            var managed = Directory.CreateDirectory(Path.Combine(root.FullName, "Managed")).FullName;
            File.WriteAllBytes(Path.Combine(managed, JobRegistrationAssembly.AssemblyName + ".dll"), JobRegistrationAssembly.Build());
            var log = new List<string>();
            var context = new AssemblyLoadContext("job-cache", isCollectible: false);
            var player = new PlayerAssemblies(managed, typeof(DisunityHost).Assembly, new[] { JobRegistrationAssembly.AssemblyName },
                callbackCache: Path.Combine(root.FullName, "cache"), log: log.Add);
            player.Attach(context);
            var assembly = context.LoadFromAssemblyName(new AssemblyName(JobRegistrationAssembly.AssemblyName));
            Expect.That(assembly.Location.StartsWith(Path.Combine(root.FullName, "cache")), $"the rewritten image loads from the cache, got {assembly.Location}");
            Register(assembly);
            Expect.Sequence(new[] { "ignores access checks to itself: JobGame" }, log, "the rewrite is logged");
        }
        finally
        {
            try { root.Delete(recursive: true); }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
                Console.Error.WriteLine($"could not remove {root.FullName}: {error.Message}");
            }
        }
    }
}
