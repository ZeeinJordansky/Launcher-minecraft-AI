namespace MintyLauncher.Services;

public interface IRefreshTokenStore
{
    bool IsSupported { get; }

    Task<string?> ReadAsync(CancellationToken cancellationToken = default);

    Task WriteAsync(string refreshToken, CancellationToken cancellationToken = default);

    Task DeleteAsync(CancellationToken cancellationToken = default);
}
