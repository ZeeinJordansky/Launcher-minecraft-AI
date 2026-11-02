using System.Security.Cryptography;

namespace MintyLauncher.Services;

public static class UpdateManifestSignatureVerifier
{
    public static bool Verify(byte[] manifest, byte[] signature, string publicKeyBase64)
    {
        ArgumentNullException.ThrowIfNull(manifest);
        ArgumentNullException.ThrowIfNull(signature);
        ArgumentException.ThrowIfNullOrWhiteSpace(publicKeyBase64);
        using var verifier = ECDsa.Create();
        verifier.ImportSubjectPublicKeyInfo(Convert.FromBase64String(publicKeyBase64), out _);
        return verifier.VerifyData(manifest, signature, HashAlgorithmName.SHA256);
    }
}
