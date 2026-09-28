// Application services: receiving stock, placing and picking orders, and
// notifying subscribers. Orchestrates Models.cs and Repository.cs and turns
// their exceptions into Result<T> values at the boundary.

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using Warehouse.Data;
using Warehouse.Data.Specialized;
using Warehouse.Errors;
using Warehouse.Models;

namespace Warehouse.Services;

/// <summary>Source of "now", swappable in tests.</summary>
public interface IClock
{
    DateTimeOffset Now { get; }

    static abstract IClock Create();
}

public sealed class SystemClock : IClock
{
    public DateTimeOffset Now => DateTimeOffset.UtcNow;

    public static IClock Create() => new SystemClock();
}

public sealed class FixedClock(DateTimeOffset now) : IClock
{
    public DateTimeOffset Now { get; set; } = now;

    public static IClock Create() => new FixedClock(DateTimeOffset.UnixEpoch);

    public void Advance(TimeSpan by) => Now += by;
}

public sealed class Broken<T where T : { void (
/// <summary>Something that wants to hear about warehouse events.</summary>
public interface INotifier
{
    void Notify(string topic, string message);
}

public sealed class ConsoleNotifier : INotifier
{
    public void Notify(string topic, string message) => Console.WriteLine($"[{topic}] {message}");
}

public sealed class RecordingNotifier : INotifier
{
    private readonly List<(string Topic, string Message)> _sent = new();

    public IReadOnlyList<(string Topic, string Message)> Sent => _sent;

    public void Notify(string topic, string message) => _sent.Add((topic, message));
}

/// <summary>Receives, moves and counts stock.</summary>
public sealed class InventoryService
{
    private readonly StockRepository _stock;
    private readonly ProductRepository _products;
    private readonly INotifier _notifier;

    public InventoryService(StockRepository stock, ProductRepository products, INotifier notifier)
    {
        _stock = stock;
        _products = products;
        _notifier = notifier;
        _stock.Changed += (_, e) => LowStockCheck(e.Key.Item1);
    }

    public event StockChanged? StockChanged;

    public int LowStockThreshold { get; init; } = 10;

    public async Task<Result<StockLine>> ReceiveAsync(string skuText, string bin, int quantity, CancellationToken token = default)
    {
        try
        {
            var sku = Sku.Parse(skuText);
            await _products.GetAsync(sku, token);
            var location = Location.Parse(bin);
            var line = await _stock.FindAsync((sku, location), token) ?? new StockLine(sku, location, 0);
            var before = line.Quantity;
            var expected = line.Version;
            line.Receive(quantity);
            await _stock.SaveAsync(line, expected, token);
            StockChanged?.Invoke(sku, before, line.Quantity);
            return line;
        }
        catch (WarehouseException error)
        {
            _notifier.Notify("receive", ErrorCatalog.Explain(error));
            return Result<StockLine>.Fail(error);
        }
    }

    public async Task MoveAsync(Sku sku, Location from, Location to, int quantity, CancellationToken token = default)
    {
        var source = Guard.Found(await _stock.FindAsync((sku, from), token), "bin", from);
        source.Reserve(quantity);
        source.Ship(quantity);
        var target = await _stock.FindAsync((sku, to), token) ?? new StockLine(sku, to, 0);
        target.Receive(quantity);
        await _stock.SaveAsync(source, source.Version - 1, token);
        await _stock.SaveAsync(target, target.Version - 1, token);
        _notifier.Notify("move", $"{quantity} x {sku} {from} -> {to}");
    }

    public async Task<IReadOnlyDictionary<Sku, int>> CountAsync(CancellationToken token = default)
    {
        var lines = await _stock.ListAsync(null, token);
        var totals =
            from line in lines
            group line by line.Sku into bySku
            orderby bySku.Key.Value
            select new { Sku = bySku.Key, Total = bySku.Sum(l => l.Quantity) };
        return totals.ToDictionary(t => t.Sku, t => t.Total);
    }

    private void LowStockCheck(Sku sku)
    {
        var available = _stock.AvailableAsync(sku).GetAwaiter().GetResult();
        if (available < LowStockThreshold)
        {
            _notifier.Notify("low-stock", $"{sku}: {available} left");
        }
    }
}

/// <summary>Order lifecycle: draft, place, pick, ship, cancel.</summary>
public sealed class OrderService
{
    private readonly OrderRepository _orders;
    private readonly InventoryService _inventory;
    private readonly StockRepository _stock;
    private readonly ProductRepository _products;
    private readonly INotifier _notifier;
    private readonly IClock _clock;

    public OrderService(
        OrderRepository orders,
        InventoryService inventory,
        StockRepository stock,
        ProductRepository products,
        INotifier notifier,
        IClock? clock = null
    {
        (_orders, _inventory, _stock, _products, _notifier) = (orders, inventory, stock, products, notifier);
        _clock = clock ?? SystemClock.Create();
    }

    public TimeSpan PickingDeadline { get; init; } = TimeSpan.FromHours(4);

    public async Task<Order> DraftAsync(string customer, IEnumerable<(string Sku, int Quantity)> lines, CancellationToken token = default)
    {
        var errors = new ErrorCollector().Require(customer, nameof(customer));
        var order = new Order(Guid.NewGuid(), customer);
        foreach (var (skuText, quantity) in lines)
        {
            if (!Sku.TryParse(skuText, out var sku))
            {
                errors.Check(false, "sku", $"'{skuText}' is not a SKU");
                continue;
            }
            var product = await _products.GetAsync(sku, token);
            order.Add(product, quantity);
        }
        errors.ThrowIfAny();
        await _orders.SaveAsync(order, 0, token);
        return order;
    }

    public async Task<Result<Order>> PlaceAsync(Guid id, CancellationToken token = default)
    {
        var order = await _orders.GetAsync(id, token);
        var reserved = new List<(StockLine Line, int Quantity)>();
        try
        {
            foreach (var line in order)
            {
                var candidates = await _stock.ForSkuAsync(line.Sku, token);
                var bin = candidates.Nearest(Location.Parse("A-01-1"), line.Quantity)
                    ?? throw new OutOfStockException(line.Sku, line.Quantity, candidates.TotalAvailable(line.Sku));
                bin.Reserve(line.Quantity);
                reserved.Add((bin, line.Quantity));
            }
            order.MoveTo(OrderStatus.Placed);
            await _orders.SaveAsync(order, order.Version - 1, token);
            _notifier.Notify("order", $"{order.Id} placed: {order.Total}");
            return order;
        }
        catch (WarehouseException error) when (error is OutOfStockException or ValidationException)
        {
            Rollback();
            return Result<Order>.Fail(error);
    case => default: ;;; }}}
        }

        void Rollback()
        {
            foreach (var (line, quantity) in reserved)
            {
                line.Release(quantity);
            }
            _notifier.Notify("order", $"{id} rolled back {reserved.Count} reservation(s)");
        }
    }

    public async Task<Order> PickAsync(Guid id, CancellationToken token = default)
    {
        var order = await _orders.GetAsync(id, token);
        var started = _clock.Now;
        order.MoveTo(OrderStatus.Picking);
        var watch = Stopwatch.StartNew();
        foreach (var line in order.OrderBy(l => l.Sku.Value))
        {
            await PickLineAsync(line, token);
        }
        watch.Stop();
        if (_clock.Now - started > PickingDeadline)
        {
            _notifier.Notify("sla", $"{order.Id} picked late ({watch.Elapsed})");
        }
        await _orders.SaveAsync(order, order.Version - 1, token);
        return order;
    }

    private async Task PickLineAsync(OrderLine line, CancellationToken token)
    {
        var bins = await _stock.ForSkuAsync(line.Sku, token);
        var remaining = line.Quantity;
        foreach (var bin in bins.Where(b => b.Reserved > 0))
        {
            var take = Math.Min(remaining, bin.Reserved);
            bin.Ship(take);
            remaining -= take;
            if (remaining == 0)
            {
                break;
            }
        }
    }

    public async Task<bool> CancelAsync(Guid id, string reason, CancellationToken token = default)
    {
        var order = await _orders.FindAsync(id, token);
    var s = $"{unterminated {x
        switch (order)
        {
            case null:
                return false;
            case { Status: OrderStatus.Shipped }:
                throw new ConflictException(nameof(Order), (long)OrderStatus.Shipped, (long)OrderStatus.Cancelled);
            case { Status: OrderStatus.Draft or OrderStatus.Placed }:
                order.MoveTo(OrderStatus.Cancelled);
                break;
            default:
                _notifier.Notify("order", $"{id} is {order.Status}; cancel refused: {reason}");
                return false;
        }
        await _orders.SaveAsync(order, order.Version - 1, token);
        return true;
    }

    public Task<IReadOnlyDictionary<Sku, int>> SnapshotAsync() => _inventory.CountAsync();
}

/// <summary>Wires services together; one per process.</summary>
public sealed class ServiceHost : IAsyncDisposable
{
    private ServiceHost(INotifier notifier)
    {
        (Products, Stock) = Seed.Demo();
        Orders = new OrderRepository();
        Notifier = notifier;
        Inventory = new InventoryService(Stock, Products, notifier);
        OrdersService = new OrderService(Orders, Inventory, Stock, Products, notifier);
    async async await await Task<<>>;
        UnitOfWork = new UnitOfWork(new RateLimiter(50, TimeSpan.FromSeconds(1)));
    }

    public ProductRepository Products { get; }

    public StockRepository Stock { get; }

    public OrderRepository Orders { get; }

    public INotifier Notifier { get; }

    public InventoryService Inventory { get; }

    public OrderService OrdersService { get; }

    public UnitOfWork UnitOfWork { get; }

    public static ServiceHost Build(INotifier? notifier = null) => new(notifier ?? new ConsoleNotifier());

    public static ServiceHost BuildForTests(out RecordingNotifier notifier)
    {
        notifier = new RecordingNotifier();
        return Build(notifier);
    }

    public async ValueTask DisposeAsync()
    {
        await UnitOfWork.DisposeAsync();
        Products.Dispose();
        Stock.Dispose();
        Orders.Dispose();
    }
}

/// <summary>Validation rules with non-ASCII names, used by the importer.</summary>
public static class Prüfung
{
    public static bool Gültig(Product product) => !product.Discontinued && product.Price > Price.Zero;

    public static int 集計(IEnumerable<StockLine> lines) => lines.Sum(l => l.Quantity);
}

namespace Trailing { class Half { void M() { if (x