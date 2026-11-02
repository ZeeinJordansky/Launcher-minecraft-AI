using System.IO.Compression;
using System.Security.Cryptography;
using MintyLauncher.Models;
using MintyLauncher.Services;
using Xunit;

namespace MintyLauncher.Tests;

public sealed class LauncherToolsServiceTests
{
    private readonly LauncherToolsService _tools = new();

    [Fact]
    public void AddProfileCreatesAnIsolatedDirectory()
    {
        var root = CreateTemporaryDirectory();
        try
        {
            var settings = new LauncherSettings { GameDirectory = root, IsolatedProfiles = true };
            _tools.AddProfile(settings, "survival-120");

            Assert.Equal("survival-120", settings.SelectedProfileId);
            Assert.Equal(Path.Combine(root, "profiles", "survival-120"), _tools.ResolveGameDirectory(settings));
            Assert.True(Directory.Exists(_tools.ResolveGameDirectory(settings)));
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }

    [Theory]
    [InlineData("../outside")]
    [InlineData("two/parts")]
    [InlineData("contains space")]
    public void AddProfileRejectsUnsafeNames(string name)
    {
        var settings = new LauncherSettings();
        Assert.Throws<ArgumentException>(() => _tools.AddProfile(settings, name));
    }

    [Fact]
    public async Task ImportWorldFindsLevelDatAndAddsSuffixForDuplicates()
    {
        var root = CreateTemporaryDirectory();
        try
        {
            var gameDirectory = Path.Combine(root, "game");
            var archivePath = Path.Combine(root, "world.zip");
            using (var archive = ZipFile.Open(archivePath, ZipArchiveMode.Create))
            {
                var entry = archive.CreateEntry("My World/level.dat");
                await using var stream = entry.Open();
                await stream.WriteAsync(new byte[] { 1, 2, 3 });
            }

            var first = await _tools.ImportWorldAsync(archivePath, gameDirectory);
            var second = await _tools.ImportWorldAsync(archivePath, gameDirectory);

            Assert.True(File.Exists(Path.Combine(first, "level.dat")));
            Assert.True(File.Exists(Path.Combine(second, "level.dat")));
            Assert.EndsWith("My World (2)", second, StringComparison.Ordinal);
            Assert.Equal(2, _tools.GetWorlds(gameDirectory).Count);
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }

    [Fact]
    public async Task ImportWorldRejectsZipSlipAndCleansStagingFiles()
    {
        var root = CreateTemporaryDirectory();
        try
        {
            var gameDirectory = Path.Combine(root, "game");
            var archivePath = Path.Combine(root, "unsafe.zip");
            using (var archive = ZipFile.Open(archivePath, ZipArchiveMode.Create))
            {
                var entry = archive.CreateEntry("../outside.txt");
                await using var stream = entry.Open();
                await stream.WriteAsync(new byte[] { 1 });
            }

            await Assert.ThrowsAsync<InvalidDataException>(() =>
                _tools.ImportWorldAsync(archivePath, gameDirectory));

            Assert.False(File.Exists(Path.Combine(root, "outside.txt")));
            Assert.Empty(Directory.EnumerateDirectories(Path.Combine(gameDirectory, "saves")));
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }

    [Fact]
    public void RemoveModCannotDeleteFilesOutsideTheModsFolder()
    {
        var root = CreateTemporaryDirectory();
        try
        {
            var outsideFile = Path.Combine(root, "important.txt");
            File.WriteAllText(outsideFile, "preserve");
            var mod = new ModFileEntry("important.txt", outsideFile, 7, false);

            Assert.Throws<InvalidOperationException>(() => _tools.RemoveMod(root, mod));
            Assert.Equal("preserve", File.ReadAllText(outsideFile));
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }

    [Fact]
    public void UpdateManifestSignatureRejectsTamperedContent()
    {
        using var signer = ECDsa.Create(ECCurve.NamedCurves.nistP256);
        var manifest = "{\"version\":\"2.1.0\"}"u8.ToArray();
        var signature = signer.SignData(manifest, HashAlgorithmName.SHA256);
        var publicKey = Convert.ToBase64String(signer.ExportSubjectPublicKeyInfo());

        Assert.True(UpdateManifestSignatureVerifier.Verify(manifest, signature, publicKey));
        manifest[0] ^= 1;
        Assert.False(UpdateManifestSignatureVerifier.Verify(manifest, signature, publicKey));
    }

    private static string CreateTemporaryDirectory()
    {
        var path = Path.Combine(Path.GetTempPath(), "minty-test-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(path);
        return path;
    }
}
