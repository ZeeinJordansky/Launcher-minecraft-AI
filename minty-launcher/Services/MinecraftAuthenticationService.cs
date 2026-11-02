using System.Net;
using System.Net.Http.Headers;
using System.Text.Json;
using System.Text.Json.Serialization.Metadata;
using MintyLauncher.Models;

namespace MintyLauncher.Services;

public sealed class MinecraftAuthenticationService : IDisposable
{
    private const string ClientId = "00000000402b5328";
    private const string DeviceCodeEndpoint =
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
    private const string TokenEndpoint =
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
    private const string XboxUserAuthEndpoint = "https://user.auth.xboxlive.com/user/authenticate";
    private const string XboxXstsEndpoint = "https://xsts.auth.xboxlive.com/xsts/authorize";
    private const string MinecraftLoginEndpoint =
        "https://api.minecraftservices.com/authentication/login_with_xbox";
    private const string MinecraftEntitlementsEndpoint =
        "https://api.minecraftservices.com/entitlements/mcstore";
    private const string MinecraftProfileEndpoint =
        "https://api.minecraftservices.com/minecraft/profile";
    private const string DeviceCodeScope = "XboxLive.signin offline_access";

    private readonly HttpClient _httpClient;
    private readonly IRefreshTokenStore _refreshTokenStore;
    private readonly TimeSpan _operationTimeout;
    private readonly bool _ownsHttpClient;

    public MinecraftAuthenticationService(
        HttpClient? httpClient = null,
        IRefreshTokenStore? refreshTokenStore = null,
        TimeSpan? operationTimeout = null)
    {
        _httpClient = httpClient ?? new HttpClient();
        _ownsHttpClient = httpClient is null;
        _refreshTokenStore = refreshTokenStore ?? new WindowsCredentialRefreshTokenStore();
        _operationTimeout = operationTimeout ?? TimeSpan.FromMinutes(15);
        if (_operationTimeout <= TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(nameof(operationTimeout));
        }
    }

    public Task<MinecraftAccount> AuthenticateAsync(
        Action<MicrosoftDeviceCodePrompt>? onDeviceCode = null,
        CancellationToken cancellationToken = default) =>
        RunWithTimeoutAsync(async token =>
        {
            EnsureCredentialStoreSupported();
            var oauth = await AcquireDeviceCodeTokenAsync(onDeviceCode, token).ConfigureAwait(false);
            var account = await AuthenticateWithMicrosoftTokenAsync(oauth.AccessToken, token).ConfigureAwait(false);
            if (string.IsNullOrWhiteSpace(oauth.RefreshToken))
            {
                throw new MinecraftAuthenticationException(
                    "Microsoft did not provide a refresh token. Please try signing in again.");
            }

            await _refreshTokenStore.WriteAsync(oauth.RefreshToken, token).ConfigureAwait(false);
            return account;
        }, cancellationToken);

    public Task<MinecraftAccount> RefreshAsync(CancellationToken cancellationToken = default) =>
        RunWithTimeoutAsync(async token =>
        {
            EnsureCredentialStoreSupported();
            var savedRefreshToken = await _refreshTokenStore.ReadAsync(token).ConfigureAwait(false);
            if (string.IsNullOrWhiteSpace(savedRefreshToken))
            {
                throw new MinecraftAuthenticationException("No saved Minecraft sign-in was found.");
            }

            var oauth = await ExchangeRefreshTokenAsync(savedRefreshToken, token).ConfigureAwait(false);
            var account = await AuthenticateWithMicrosoftTokenAsync(oauth.AccessToken, token).ConfigureAwait(false);
            var refreshToken = string.IsNullOrWhiteSpace(oauth.RefreshToken)
                ? savedRefreshToken
                : oauth.RefreshToken;
            await _refreshTokenStore.WriteAsync(refreshToken, token).ConfigureAwait(false);
            return account;
        }, cancellationToken);

    public Task SignOutAsync(CancellationToken cancellationToken = default) =>
        RunWithTimeoutAsync(async token =>
        {
            EnsureCredentialStoreSupported();
            await _refreshTokenStore.DeleteAsync(token).ConfigureAwait(false);
            return true;
        }, cancellationToken);

    public void Dispose()
    {
        if (_ownsHttpClient)
        {
            _httpClient.Dispose();
        }
    }

    private async Task<OAuthTokenResponse> AcquireDeviceCodeTokenAsync(
        Action<MicrosoftDeviceCodePrompt>? onDeviceCode,
        CancellationToken cancellationToken)
    {
        using var request = new HttpRequestMessage(HttpMethod.Post, DeviceCodeEndpoint)
        {
            Content = new FormUrlEncodedContent(new Dictionary<string, string>
            {
                ["client_id"] = ClientId,
                ["scope"] = DeviceCodeScope
            })
        };
        using var response = await SendAsync(request, cancellationToken).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw new MinecraftAuthenticationException("Microsoft sign-in could not be started.");
        }

        var deviceCode = await ReadJsonAsync(response, MinecraftAuthenticationJsonContext.Default.DeviceCodeResponse,
            "Microsoft sign-in could not be started.", cancellationToken).ConfigureAwait(false);
        if (string.IsNullOrWhiteSpace(deviceCode.DeviceCode) ||
            string.IsNullOrWhiteSpace(deviceCode.UserCode) ||
            !Uri.TryCreate(deviceCode.VerificationUri, UriKind.Absolute, out var verificationUri) ||
            verificationUri.Scheme != Uri.UriSchemeHttps)
        {
            throw new MinecraftAuthenticationException("Microsoft returned an invalid sign-in response.");
        }

        var intervalSeconds = Math.Clamp(deviceCode.Interval <= 0 ? 5 : deviceCode.Interval, 1, 60);
        var expiresInSeconds = Math.Clamp(deviceCode.ExpiresIn, 1, 3600);
        var expiresAt = DateTimeOffset.UtcNow.AddSeconds(expiresInSeconds);
        onDeviceCode?.Invoke(new MicrosoftDeviceCodePrompt(
            verificationUri.ToString(),
            deviceCode.UserCode,
            expiresAt,
            intervalSeconds));

        while (DateTimeOffset.UtcNow < expiresAt)
        {
            var remaining = expiresAt - DateTimeOffset.UtcNow;
            if (remaining <= TimeSpan.Zero)
            {
                break;
            }
            await Task.Delay(
                remaining < TimeSpan.FromSeconds(intervalSeconds)
                    ? remaining
                    : TimeSpan.FromSeconds(intervalSeconds),
                cancellationToken).ConfigureAwait(false);
            if (DateTimeOffset.UtcNow >= expiresAt)
            {
                break;
            }
            using var tokenRequest = new HttpRequestMessage(HttpMethod.Post, TokenEndpoint)
            {
                Content = new FormUrlEncodedContent(new Dictionary<string, string>
                {
                    ["grant_type"] = "urn:ietf:params:oauth:grant-type:device_code",
                    ["client_id"] = ClientId,
                    ["device_code"] = deviceCode.DeviceCode
                })
            };
            using var tokenResponse = await SendAsync(tokenRequest, cancellationToken).ConfigureAwait(false);
            if (tokenResponse.IsSuccessStatusCode)
            {
                return await ReadJsonAsync(tokenResponse, MinecraftAuthenticationJsonContext.Default.OAuthTokenResponse,
                    "Microsoft sign-in could not be completed.", cancellationToken).ConfigureAwait(false);
            }

            var oauthError = await ReadOAuthErrorAsync(tokenResponse, cancellationToken).ConfigureAwait(false);
            switch (oauthError)
            {
                case "authorization_pending":
                    continue;
                case "slow_down":
                    intervalSeconds = Math.Min(intervalSeconds + 5, 60);
                    continue;
                case "authorization_declined":
                    throw new MinecraftAuthenticationException("Microsoft sign-in was declined.");
                case "expired_token":
                case "bad_verification_code":
                    throw new MinecraftAuthenticationException("The Microsoft sign-in code expired.");
                default:
                    throw new MinecraftAuthenticationException("Microsoft sign-in could not be completed.");
            }
        }

        throw new MinecraftAuthenticationException("The Microsoft sign-in code expired.");
    }

    private async Task<OAuthTokenResponse> ExchangeRefreshTokenAsync(
        string refreshToken,
        CancellationToken cancellationToken)
    {
        using var request = new HttpRequestMessage(HttpMethod.Post, TokenEndpoint)
        {
            Content = new FormUrlEncodedContent(new Dictionary<string, string>
            {
                ["client_id"] = ClientId,
                ["grant_type"] = "refresh_token",
                ["refresh_token"] = refreshToken,
                ["scope"] = DeviceCodeScope
            })
        };
        using var response = await SendAsync(request, cancellationToken).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw new MinecraftAuthenticationException(
                "The saved Microsoft sign-in has expired. Please sign in again.");
        }

        return await ReadJsonAsync(response, MinecraftAuthenticationJsonContext.Default.OAuthTokenResponse,
            "The saved Microsoft sign-in could not be refreshed.", cancellationToken).ConfigureAwait(false);
    }

    private async Task<MinecraftAccount> AuthenticateWithMicrosoftTokenAsync(
        string microsoftAccessToken,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(microsoftAccessToken))
        {
            throw new MinecraftAuthenticationException("Microsoft returned an invalid sign-in response.");
        }

        var xboxRequest = new XboxUserAuthRequest();
        xboxRequest.Properties.RpsTicket = $"d={microsoftAccessToken}";
        var xboxResponse = await PostJsonAsync(
            XboxUserAuthEndpoint,
            xboxRequest,
            MinecraftAuthenticationJsonContext.Default.XboxUserAuthRequest,
            MinecraftAuthenticationJsonContext.Default.XboxTokenResponse,
            "Xbox sign-in failed.",
            cancellationToken).ConfigureAwait(false);

        var userHash = xboxResponse.DisplayClaims.Users.FirstOrDefault()?.UserHash;
        var xuid = xboxResponse.DisplayClaims.Users.FirstOrDefault()?.Xuid ?? string.Empty;
        if (string.IsNullOrWhiteSpace(xboxResponse.Token) || string.IsNullOrWhiteSpace(userHash))
        {
            throw new MinecraftAuthenticationException("Xbox sign-in returned an invalid response.");
        }

        var xstsRequest = new XstsAuthRequest();
        xstsRequest.Properties.UserTokens.Add(xboxResponse.Token);
        var xstsResponse = await PostJsonAsync(
            XboxXstsEndpoint,
            xstsRequest,
            MinecraftAuthenticationJsonContext.Default.XstsAuthRequest,
            MinecraftAuthenticationJsonContext.Default.XboxTokenResponse,
            "Xbox sign-in could not be authorized.",
            cancellationToken).ConfigureAwait(false);

        if (string.IsNullOrWhiteSpace(xstsResponse.Token))
        {
            throw new MinecraftAuthenticationException("Xbox sign-in returned an invalid response.");
        }

        var minecraftToken = await PostJsonAsync(
            MinecraftLoginEndpoint,
            new MinecraftLoginRequest { IdentityToken = $"XBL3.0 x={userHash};{xstsResponse.Token}" },
            MinecraftAuthenticationJsonContext.Default.MinecraftLoginRequest,
            MinecraftAuthenticationJsonContext.Default.MinecraftTokenResponse,
            "Minecraft sign-in failed.",
            cancellationToken).ConfigureAwait(false);

        if (string.IsNullOrWhiteSpace(minecraftToken.AccessToken))
        {
            throw new MinecraftAuthenticationException("Minecraft returned an invalid sign-in response.");
        }

        using var entitlementsRequest = new HttpRequestMessage(HttpMethod.Get, MinecraftEntitlementsEndpoint);
        entitlementsRequest.Headers.Authorization =
            new AuthenticationHeaderValue("Bearer", minecraftToken.AccessToken);
        using var entitlementsResponse = await SendAsync(entitlementsRequest, cancellationToken).ConfigureAwait(false);
        if (!entitlementsResponse.IsSuccessStatusCode)
        {
            throw new MinecraftAuthenticationException("Minecraft ownership could not be verified.");
        }

        var entitlements = await ReadJsonAsync(
            entitlementsResponse,
            MinecraftAuthenticationJsonContext.Default.MinecraftEntitlementsResponse,
            "Minecraft ownership could not be verified.",
            cancellationToken).ConfigureAwait(false);
        if (!entitlements.Items.Any(item =>
                item.Name is "game_minecraft" or "product_minecraft"))
        {
            throw new MinecraftAuthenticationException(
                "This Microsoft account does not own Minecraft: Java Edition.");
        }

        using var profileRequest = new HttpRequestMessage(HttpMethod.Get, MinecraftProfileEndpoint);
        profileRequest.Headers.Authorization = new AuthenticationHeaderValue("Bearer", minecraftToken.AccessToken);
        using var profileResponse = await SendAsync(profileRequest, cancellationToken).ConfigureAwait(false);
        if (profileResponse.StatusCode == HttpStatusCode.NotFound)
        {
            throw new MinecraftAuthenticationException(
                "This Microsoft account does not have a Minecraft Java profile.");
        }
        if (!profileResponse.IsSuccessStatusCode)
        {
            throw new MinecraftAuthenticationException("The Minecraft profile could not be verified.");
        }

        var profile = await ReadJsonAsync(
            profileResponse,
            MinecraftAuthenticationJsonContext.Default.MinecraftProfileResponse,
            "The Minecraft profile could not be verified.",
            cancellationToken).ConfigureAwait(false);
        if (string.IsNullOrWhiteSpace(profile.Id) || string.IsNullOrWhiteSpace(profile.Name))
        {
            throw new MinecraftAuthenticationException("Minecraft returned an invalid profile.");
        }

        return new MinecraftAccount(profile.Id, profile.Name, minecraftToken.AccessToken, xuid);
    }

    private async Task<TResponse> PostJsonAsync<TRequest, TResponse>(
        string endpoint,
        TRequest value,
        JsonTypeInfo<TRequest> requestTypeInfo,
        JsonTypeInfo<TResponse> responseTypeInfo,
        string failureMessage,
        CancellationToken cancellationToken)
    {
        var payload = JsonSerializer.SerializeToUtf8Bytes(value, requestTypeInfo);
        using var request = new HttpRequestMessage(HttpMethod.Post, endpoint)
        {
            Content = new ByteArrayContent(payload)
        };
        request.Content.Headers.ContentType = new MediaTypeHeaderValue("application/json");
        using var response = await SendAsync(request, cancellationToken).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw new MinecraftAuthenticationException(failureMessage);
        }

        return await ReadJsonAsync(response, responseTypeInfo, failureMessage, cancellationToken)
            .ConfigureAwait(false);
    }

    private async Task<HttpResponseMessage> SendAsync(
        HttpRequestMessage request,
        CancellationToken cancellationToken)
    {
        try
        {
            return await _httpClient.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (HttpRequestException)
        {
            throw new MinecraftAuthenticationException("A sign-in service is temporarily unavailable.");
        }
    }

    private static async Task<T> ReadJsonAsync<T>(
        HttpResponseMessage response,
        JsonTypeInfo<T> typeInfo,
        string failureMessage,
        CancellationToken cancellationToken)
    {
        try
        {
            await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
            return await JsonSerializer.DeserializeAsync(stream, typeInfo, cancellationToken).ConfigureAwait(false)
                ?? throw new MinecraftAuthenticationException(failureMessage);
        }
        catch (JsonException)
        {
            throw new MinecraftAuthenticationException(failureMessage);
        }
    }

    private static async Task<string?> ReadOAuthErrorAsync(
        HttpResponseMessage response,
        CancellationToken cancellationToken)
    {
        try
        {
            await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
            var error = await JsonSerializer.DeserializeAsync(
                stream,
                MinecraftAuthenticationJsonContext.Default.OAuthTokenResponse,
                cancellationToken).ConfigureAwait(false);
            return error?.Error;
        }
        catch (JsonException)
        {
            return null;
        }
    }

    private void EnsureCredentialStoreSupported()
    {
        if (!_refreshTokenStore.IsSupported)
        {
            throw new PlatformNotSupportedException(
                "Minecraft sign-in is currently supported only on Windows because secure credential storage is required.");
        }
    }

    private async Task<T> RunWithTimeoutAsync<T>(
        Func<CancellationToken, Task<T>> operation,
        CancellationToken cancellationToken)
    {
        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(_operationTimeout);
        try
        {
            return await operation(timeoutSource.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new TimeoutException("Minecraft sign-in timed out. Please try again.");
        }
    }
}

public sealed class MinecraftAuthenticationException : Exception
{
    public MinecraftAuthenticationException(string message) : base(message)
    {
    }
}
