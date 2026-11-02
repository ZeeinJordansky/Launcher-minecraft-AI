using System.Text.Json.Serialization;

namespace MintyLauncher.Models;

public sealed record MinecraftAccount(string Id, string Username, string AccessToken, string Xuid);

public sealed record MicrosoftDeviceCodePrompt(
    string VerificationUri,
    string UserCode,
    DateTimeOffset ExpiresAt,
    int PollingIntervalSeconds);

internal sealed class DeviceCodeResponse
{
    [JsonPropertyName("device_code")]
    public string DeviceCode { get; set; } = string.Empty;

    [JsonPropertyName("user_code")]
    public string UserCode { get; set; } = string.Empty;

    [JsonPropertyName("verification_uri")]
    public string VerificationUri { get; set; } = string.Empty;

    [JsonPropertyName("expires_in")]
    public int ExpiresIn { get; set; }

    [JsonPropertyName("interval")]
    public int Interval { get; set; }
}

internal sealed class OAuthTokenResponse
{
    [JsonPropertyName("access_token")]
    public string AccessToken { get; set; } = string.Empty;

    [JsonPropertyName("refresh_token")]
    public string? RefreshToken { get; set; }

    [JsonPropertyName("error")]
    public string? Error { get; set; }
}

internal sealed class XboxUserAuthRequest
{
    [JsonPropertyName("Properties")]
    public XboxUserAuthProperties Properties { get; set; } = new();

    [JsonPropertyName("RelyingParty")]
    public string RelyingParty { get; set; } = "http://auth.xboxlive.com";

    [JsonPropertyName("TokenType")]
    public string TokenType { get; set; } = "JWT";
}

internal sealed class XboxUserAuthProperties
{
    [JsonPropertyName("AuthMethod")]
    public string AuthMethod { get; set; } = "RPS";

    [JsonPropertyName("SiteName")]
    public string SiteName { get; set; } = "user.auth.xboxlive.com";

    [JsonPropertyName("RpsTicket")]
    public string RpsTicket { get; set; } = string.Empty;
}

internal sealed class XboxTokenResponse
{
    [JsonPropertyName("Token")]
    public string Token { get; set; } = string.Empty;

    [JsonPropertyName("DisplayClaims")]
    public XboxDisplayClaims DisplayClaims { get; set; } = new();
}

internal sealed class XboxDisplayClaims
{
    [JsonPropertyName("xui")]
    public List<XboxUserClaim> Users { get; set; } = [];
}

internal sealed class XboxUserClaim
{
    [JsonPropertyName("uhs")]
    public string UserHash { get; set; } = string.Empty;

    [JsonPropertyName("xid")]
    public string Xuid { get; set; } = string.Empty;
}

internal sealed class XstsAuthRequest
{
    [JsonPropertyName("Properties")]
    public XstsAuthProperties Properties { get; set; } = new();

    [JsonPropertyName("RelyingParty")]
    public string RelyingParty { get; set; } = "rp://api.minecraftservices.com/";

    [JsonPropertyName("TokenType")]
    public string TokenType { get; set; } = "JWT";
}

internal sealed class XstsAuthProperties
{
    [JsonPropertyName("SandboxId")]
    public string SandboxId { get; set; } = "RETAIL";

    [JsonPropertyName("UserTokens")]
    public List<string> UserTokens { get; set; } = [];
}

internal sealed class MinecraftLoginRequest
{
    [JsonPropertyName("identityToken")]
    public string IdentityToken { get; set; } = string.Empty;
}

internal sealed class MinecraftTokenResponse
{
    [JsonPropertyName("access_token")]
    public string AccessToken { get; set; } = string.Empty;
}

internal sealed class MinecraftEntitlementsResponse
{
    [JsonPropertyName("items")]
    public List<MinecraftEntitlement> Items { get; set; } = [];
}

internal sealed class MinecraftEntitlement
{
    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;
}

internal sealed class MinecraftProfileResponse
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;
}

[JsonSourceGenerationOptions(GenerationMode = JsonSourceGenerationMode.Metadata)]
[JsonSerializable(typeof(DeviceCodeResponse))]
[JsonSerializable(typeof(OAuthTokenResponse))]
[JsonSerializable(typeof(XboxUserAuthRequest))]
[JsonSerializable(typeof(XboxTokenResponse))]
[JsonSerializable(typeof(XstsAuthRequest))]
[JsonSerializable(typeof(MinecraftLoginRequest))]
[JsonSerializable(typeof(MinecraftTokenResponse))]
[JsonSerializable(typeof(MinecraftEntitlementsResponse))]
[JsonSerializable(typeof(MinecraftProfileResponse))]
internal partial class MinecraftAuthenticationJsonContext : JsonSerializerContext;
