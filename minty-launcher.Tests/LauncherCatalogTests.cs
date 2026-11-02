using System.Net;
using System.Text;
using MintyLauncher.Services;
using Xunit;

namespace MintyLauncher.Tests;

public sealed class LauncherCatalogTests
{
    [Fact]
    public async Task VersionCatalogReturnsEveryReleaseAndSnapshot()
    {
        var versions = Enumerable.Range(0, 120)
            .Select(index => new
            {
                id = $"1.{index}",
                type = index == 0 ? "snapshot" : "release",
                releaseTime = "2024-01-01T00:00:00+00:00"
            });
        var manifest = System.Text.Json.JsonSerializer.Serialize(new { versions });
        using var client = CreateClient(manifest);
        using var launcher = new MinecraftLauncherService(client);

        var result = await launcher.GetVersionsAsync();

        Assert.Equal(120, result.Count);
        Assert.Contains(result, version => version.Type == "snapshot");
        Assert.Contains(result, version => version.Id == "1.119");
    }

    [Fact]
    public async Task ModCatalogCanBrowsePopularProjectsWithoutAQueryOrLoader()
    {
        var handler = new StaticResponseHandler("""{"hits":[]}""");
        using var client = new HttpClient(handler);
        using var modrinth = new ModrinthService(client);

        var result = await modrinth.SearchAsync(
            string.Empty, "all", "1.21.1", "Vanilla", "mod", "downloads");

        Assert.Empty(result);
        Assert.NotNull(handler.RequestUri);
        Assert.Contains("query=&", handler.RequestUri.Query);
        Assert.Contains("index=downloads", handler.RequestUri.Query);
    }

    private static HttpClient CreateClient(string content) =>
        new(new StaticResponseHandler(content));

    private sealed class StaticResponseHandler(string content) : HttpMessageHandler
    {
        public Uri? RequestUri { get; private set; }

        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken)
        {
            RequestUri = request.RequestUri;
            return Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(content, Encoding.UTF8, "application/json")
            });
        }
    }
}
