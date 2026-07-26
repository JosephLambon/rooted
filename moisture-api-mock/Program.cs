var app = WebApplication.CreateBuilder(args).Build();

app.MapPost("/moisture", (Moisture m) =>
{
    Console.WriteLine($"[{DateTime.Now:HH:mm:ss}] moisture = {m.moisture}");
    return Results.Ok();
});

app.Run("http://0.0.0.0:8080");   // 0.0.0.0 = listen on LAN, not just localhost

record Moisture(int moisture);
