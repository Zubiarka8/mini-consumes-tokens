// Command-line front end: top-level statements parse the arguments and
// dispatch to one of the Command classes below, which call the services
// and reports. Exit code 0 on success, the error's HTTP status otherwise.

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using Warehouse.Cli;
using Warehouse.Errors;
using Warehouse.Models;
using Warehouse.Reporting;
using Warehouse.Services;

var cancel = new CancellationTokenSource();
Console.CancelKeyPress += (_, e) =>
{
    e.Cancel = true;
    cancel.Cancel();
};

await using var host = ServiceHost.Build();
var registry = CommandRegistry.Default(host);
var exitCode = await RunAsync(args, cancel.Token);
return exitCode;

async Task<int> RunAsync(string[] argv, CancellationToken token)
{
    if (argv.Length == 0 || argv[0] is "-h" or "--help")
    {
        PrintUsage();
        return 0;
    }
    var command = registry.Find(argv[0]);
    if (command is null)
    {
        Console.Error.WriteLine($"unknown command '{argv[0]}'");
        PrintUsage();
        return 2;
    }
    try
    {
        var options = Options.Parse(argv.Skip(1));
        return await command.RunAsync(options, token);
    }
    catch (WarehouseException error)
    {
        Console.Error.WriteLine(ErrorCatalog.Explain(error));
        return ErrorCatalog.Http.StatusFor(error.Code) / 100;
    }
    catch (OperationCanceledException)
    {
        Console.Error.WriteLine("cancelled");
        return 130;
    }
}

void PrintUsage()
{
    Console.WriteLine("usage: warehouse <command> [--key value]...");
    foreach (var (name, summary) in registry.Describe())
    {
        Console.WriteLine($"  {name,-10} {summary}");
    }
}

namespace Warehouse.Cli
{
    /// <summary>Names a command and gives its one-line help.</summary>
    [AttributeUsage(AttributeTargets.Class)]
    public sealed class CommandAttribute : Attribute
    {
        public CommandAttribute(string name, string summary)
        {
            Name = name;
            Summary = summary;
        }

        public string Name { get; }

        public string Summary { get; }
    }

    /// <summary>Parsed <c>--key value</c> pairs plus positional words.</summary>
    public sealed class Options
    {
        private readonly Dictionary<string, string> _named = new(StringComparer.OrdinalIgnoreCase);
        private readonly List<string> _positional = new();

        public IReadOnlyList<string> Positional => _positional;

        public static Options Parse(IEnumerable<string> words)
        {
            var options = new Options();
            string? pending = null;
            foreach (var word in words)
            {
                if (word.StartsWith("--", StringComparison.Ordinal))
                {
                    if (pending is not null)
                    {
                        options._named[pending] = "true";
                    }
                    pending = word[2..];
                }
                else if (pending is not null)
                {
                    options._named[pending] = word;
                    pending = null;
                }
                else
                {
                    options._positional.Add(word);
                }
            }
            if (pending is not null)
            {
                options._named[pending] = "true";
            }
            return options;
        }

        public string Required(string key) =>
            _named.TryGetValue(key, out var value) ? value : throw new ValidationException(key, "is required");

        public string Get(string key, string fallback) => _named.GetValueOrDefault(key, fallback);

        public int Int(string key, int fallback)
        {
            if (!_named.TryGetValue(key, out var raw))
            {
                return fallback;
            }
            return int.TryParse(raw, out var value) ? value : throw new ValidationException(key, $"'{raw}' is not a number");
        }

        public bool Flag(string key) => _named.ContainsKey(key);
    }

    /// <summary>Base of every command.</summary>
    public abstract class Command
    {
        protected Command(ServiceHost host) => Host = host;

        protected ServiceHost Host { get; }

        public string Name => Metadata?.Name ?? GetType().Name.Replace("Command", "").ToLowerInvariant();

        public string Summary => Metadata?.Summary ?? "";

        private CommandAttribute? Metadata =>
            (CommandAttribute?)Attribute.GetCustomAttribute(GetType(), typeof(CommandAttribute));

        public abstract Task<int> RunAsync(Options options, CancellationToken token);

        protected void Say(string message) => Host.Notifier.Notify(Name, message);
    }

    [Command("receive", "receive stock into a bin: --sku --bin --qty")]
    public sealed class ReceiveCommand : Command
    {
        public ReceiveCommand(ServiceHost host)
            : base(host)
        {
        }

        public override async Task<int> RunAsync(Options options, CancellationToken token)
        {
            var result = await Host.Inventory.ReceiveAsync(
                options.Required("sku"),
                options.Get("bin", "A-01-1"),
                options.Int("qty", 1),
                token);
            var (ok, line, error) = result;
            if (!ok)
            {
                Say(ErrorCatalog.Explain(error!));
                return 1;
            }
            Say($"{line!.Sku} now {line.Quantity} in {line.Bin}");
            return 0;
        }
    }

    [Command("order", "draft and place an order: --customer, then SKU=QTY words")]
    public sealed class OrderCommand : Command
    {
        public OrderCommand(ServiceHost host)
            : base(host)
        {
        }

        public override async Task<int> RunAsync(Options options, CancellationToken token)
        {
            var lines = options.Positional.Select(ParseLine).ToList();
            var order = await Host.OrdersService.DraftAsync(options.Required("customer"), lines, token);
            var placed = await Host.OrdersService.PlaceAsync(order.Id, token);
            if (!placed.IsOk)
            {
                Say($"not placed: {placed.Error?.Describe()}");
                return 1;
            }
            if (options.Flag("pick"))
            {
                await Host.OrdersService.PickAsync(order.Id, token);
            }
            Say($"order {order.Id} for {order.Customer}: {order.Total}");
            return 0;
        }

        private static (string Sku, int Quantity) ParseLine(string word)
        {
            var parts = word.Split('=', 2);
            var quantity = parts.Length == 2 && int.TryParse(parts[1], out var q) ? q : 1;
            return (parts[0], quantity);
        }
    }

    [Command("cancel", "cancel an order: --id --reason")]
    public sealed class CancelCommand(ServiceHost host) : Command(host)
    {
        public override async Task<int> RunAsync(Options options, CancellationToken token)
        {
            var id = Guid.Parse(options.Required("id"));
            var cancelled = await Host.OrdersService.CancelAsync(id, options.Get("reason", "customer request"), token);
            Say(cancelled ? $"{id} cancelled" : $"{id} not cancelled");
            return cancelled ? 0 : 1;
        }
    }

    [Command("report", "print a report: inventory | backlog | summary, --format text|csv")]
    public sealed class ReportCommand : Command
    {
        public ReportCommand(ServiceHost host)
            : base(host)
        {
        }

        public override async Task<int> RunAsync(Options options, CancellationToken token)
        {
            token.ThrowIfCancellationRequested();
            var format = options.Get("format", "text");
            var which = options.Positional.FirstOrDefault() ?? "summary";
            var output = which switch
            {
                "inventory" => ReportBuilder.Render<InventoryRow>(await ReportBuilder.InventoryAsync(Host), format),
                "backlog" => ReportBuilder.Render(await ReportBuilder.BacklogAsync(Host.Orders), format),
                "summary" => (string)await Summary.ComputeAsync(Host),
                _ => throw new ValidationException("report", $"unknown report '{which}'"),
            };
            Console.WriteLine(output);
            if (options.Flag("checksum"))
            {
                Console.WriteLine($"checksum {ReportBuilder.Checksum(output):x8}");
            }
            return 0;
        }
    }

    [Command("tags", "list products with a tag: --tag")]
    public sealed class TagsCommand : Command
    {
        public TagsCommand(ServiceHost host)
            : base(host)
        {
        }

        public override async Task<int> RunAsync(Options options, CancellationToken token)
        {
            var products = await Host.Products.ByTagAsync(options.Required("tag"), token);
            foreach (var product in products)
            {
                Console.WriteLine($"{product.Id}  {product.Describe().Truncate(40)}  {product.Price}");
            }
            return products.Count == 0 ? 1 : 0;
        }
    }

    /// <summary>All commands, looked up by name.</summary>
    public sealed class CommandRegistry
    {
        private readonly Dictionary<string, Command> _commands = new(StringComparer.OrdinalIgnoreCase);

        public static CommandRegistry Default(ServiceHost host) => new CommandRegistry()
            .Add(new ReceiveCommand(host))
            .Add(new OrderCommand(host))
            .Add(new CancelCommand(host))
            .Add(new ReportCommand(host))
            .Add(new TagsCommand(host));

        public CommandRegistry Add(Command command)
        {
            _commands[command.Name] = command;
            return this;
        }

        public Command? Find(string name) => _commands.GetValueOrDefault(name);

        public IEnumerable<(string Name, string Summary)> Describe() =>
            _commands.Values.OrderBy(c => c.Name).Select(c => (c.Name, c.Summary));
    }
}
