// Domain model: SKUs, products, stock lines, orders and the value objects
// they are built from. Validation goes through Warehouse.Errors.Guard so
// every layer reports the same error codes.

using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using Warehouse.Errors;
using Money = System.Decimal;

namespace Warehouse.Models;

public enum Unit
{
    Piece,
    Box,
    Pallet,
    Kilogram,
}

public enum OrderStatus
{
    Draft,
    Placed,
    Picking,
    Shipped,
    Cancelled,
}

/// <summary>Stock-keeping unit: three letters, a dash, four digits.</summary>
public readonly record struct Sku
{
    public Sku(string value)
    {
        var text = Guard.NotBlank(value).ToUpperInvariant();
        if (!IsValid(text))
        {
            throw new ValidationException("sku", $"'{value}' is not AAA-0000");
        }
        Value = text;
    }

    public string Value { get; }

    public string Family => Value[..3];

    public static bool IsValid(string text) =>
        text.Length == 8 && text[3] == '-' && text[..3].All(char.IsLetter) && text[4..].All(char.IsDigit);

    public static Sku Parse(string text) => new(text);

    public static bool TryParse(string? text, out Sku sku)
    {
        sku = default;
        if (text is null || !IsValid(text.Trim().ToUpperInvariant()))
        {
            return false;
        }
        sku = new Sku(text);
        return true;
    }

    public override string ToString() => Value;

    public static implicit operator string(Sku sku) => sku.Value;
}

/// <summary>An amount of money in one currency.</summary>
public readonly record struct Price(Money Amount, string Currency = "EUR")
{
    public static readonly Price Zero = new(0m);

    public Price Validated() => this with { Amount = Amount.InRange(0m, 1_000_000m, "price") };

    public static Price operator +(Price a, Price b)
    {
        SameCurrency(a, b);
        return new Price(a.Amount + b.Amount, a.Currency);
    }

    public static Price operator *(Price a, int quantity) => new(a.Amount * quantity, a.Currency);

    public static bool operator >(Price a, Price b) => Compare(a, b) > 0;

    public static bool operator <(Price a, Price b) => Compare(a, b) < 0;

    private static int Compare(Price a, Price b)
    {
        SameCurrency(a, b);
        return a.Amount.CompareTo(b.Amount);
    }

    private static void SameCurrency(Price a, Price b)
    {
        if (a.Currency != b.Currency)
        {
            throw new ValidationException("currency", $"{a.Currency} vs {b.Currency}");
        }
    }

    public override string ToString() => $"{Amount:0.00} {Currency}";
}

/// <summary>Anything stored with an optimistic-concurrency version.</summary>
public interface IEntity<out TKey>
    where TKey : notnull
{
    TKey Id { get; }

    long Version { get; }

    bool IsNew => Version == 0;
}

public interface IAuditable
{
    DateTimeOffset CreatedAt { get; }

    DateTimeOffset? UpdatedAt { get; set; }

    void Touch() => UpdatedAt = DateTimeOffset.UtcNow;
}

/// <summary>Base for mutable entities.</summary>
public abstract class Entity<TKey> : IEntity<TKey>, IAuditable
    where TKey : notnull
{
    protected Entity(TKey id)
    {
        Id = id;
        CreatedAt = DateTimeOffset.UtcNow;
    }

    public TKey Id { get; }

    public long Version { get; internal set; }

    public DateTimeOffset CreatedAt { get; }

    public DateTimeOffset? UpdatedAt { get; set; }

    internal void Bump()
    {
        Version++;
        ((IAuditable)this).Touch();
    }

    public override bool Equals(object? obj) =>
        obj is Entity<TKey> other && EqualityComparer<TKey>.Default.Equals(Id, other.Id);

    public override int GetHashCode() => Id.GetHashCode();
}

/// <summary>A catalogue product.</summary>
public sealed class Product : Entity<Sku>
{
    private readonly HashSet<string> _tags = new(StringComparer.OrdinalIgnoreCase);

    public Product(Sku sku, string name, Price price, Unit unit = Unit.Piece)
        : base(sku)
    {
        Name = Guard.NotBlank(name);
        Price = price.Validated();
        Unit = unit;
    }

    public string Name { get; private set; }

    public Price Price { get; private set; }

    public Unit Unit { get; }

    public IReadOnlyCollection<string> Tags => _tags;

    public bool Discontinued { get; private set; }

    public Product Rename(string name)
    {
        Name = Guard.NotBlank(name);
        Bump();
        return this;
    }

    public Product Reprice(Price price)
    {
        Price = price.Validated();
        Bump();
        return this;
    }

    public Product Tag(params string[] tags)
    {
        foreach (var tag in tags.Where(t => !string.IsNullOrWhiteSpace(t)))
        {
            _tags.Add(tag.Trim());
        }
        return this;
    }

    public void Discontinue() => (Discontinued, UpdatedAt) = (true, DateTimeOffset.UtcNow);
}

/// <summary>A bin in the warehouse: aisle, rack and shelf.</summary>
public sealed record Location(char Aisle, int Rack, int Shelf) : IComparable<Location>
{
    public static Location Parse(string code)
    {
        var parts = Guard.NotBlank(code).Split('-');
        if (parts.Length != 3 || parts[0].Length != 1)
        {
            throw new ValidationException("location", $"'{code}' is not A-00-0");
        }
        return new Location(parts[0][0], int.Parse(parts[1]), int.Parse(parts[2]));
    }

    public int CompareTo(Location? other) => other is null
        ? 1
        : (Aisle, Rack, Shelf).CompareTo((other.Aisle, other.Rack, other.Shelf));

    public int Distance(Location other) =>
        Math.Abs(Aisle - other.Aisle) * 20 + Math.Abs(Rack - other.Rack) + Math.Abs(Shelf - other.Shelf);

    public override string ToString() => $"{Aisle}-{Rack:00}-{Shelf}";
}

/// <summary>How many units of one SKU sit in one bin.</summary>
public sealed class StockLine(Sku sku, Location bin, int quantity) : Entity<(Sku, Location)>((sku, bin))
{
    public Sku Sku { get; } = sku;

    public Location Bin { get; } = bin;

    public int Quantity { get; private set; } = quantity;

    public int Reserved { get; private set; }

    public int Available => Quantity - Reserved;

    public void Receive(int amount)
    {
        Quantity += Guard.Positive(amount, nameof(amount));
        Bump();
    }

    public void Reserve(int amount)
    {
        if (Guard.Positive(amount, nameof(amount)) > Available)
        {
            throw new OutOfStockException(Sku, amount, Available);
        }
        Reserved += amount;
        Bump();
    }

    public void Release(int amount)
    {
        Reserved = Math.Max(0, Reserved - amount);
        Bump();
    }

    public void Ship(int amount)
    {
        Release(amount);
        Quantity -= amount;
        Bump();
    }
}

/// <summary>One product and quantity on an order.</summary>
public sealed record OrderLine(Sku Sku, int Quantity, Price UnitPrice)
{
    public Price Total => UnitPrice * Quantity;
}

/// <summary>A customer order; lines are indexable by SKU.</summary>
public sealed partial class Order : Entity<Guid>, IEnumerable<OrderLine>
{
    private readonly List<OrderLine> _lines = new();

    public Order(Guid id, string customer)
        : base(id)
    {
        Customer = Guard.NotBlank(customer);
    }

    public string Customer { get; }

    public OrderStatus Status { get; private set; } = OrderStatus.Draft;

    public OrderLine? this[Sku sku] => _lines.FirstOrDefault(l => l.Sku == sku);

    public int Count => _lines.Count;

    public Price Total => _lines.Aggregate(Price.Zero, (sum, line) => sum + line.Total);

    public Order Add(Product product, int quantity)
    {
        EnsureStatus(OrderStatus.Draft);
        if (product.Discontinued)
        {
            throw new ValidationException("product", $"{product.Id} is discontinued");
        }
        var existing = this[product.Id];
        if (existing is not null)
        {
            _lines.Remove(existing);
            quantity += existing.Quantity;
        }
        _lines.Add(new OrderLine(product.Id, Guard.Positive(quantity, "quantity"), product.Price));
        Bump();
        return this;
    }

    public IEnumerator<OrderLine> GetEnumerator() => _lines.GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}

public sealed partial class Order
{
    private static readonly Dictionary<OrderStatus, OrderStatus[]> Transitions = new()
    {
        [OrderStatus.Draft] = new[] { OrderStatus.Placed, OrderStatus.Cancelled },
        [OrderStatus.Placed] = new[] { OrderStatus.Picking, OrderStatus.Cancelled },
        [OrderStatus.Picking] = new[] { OrderStatus.Shipped },
    };

    public void MoveTo(OrderStatus next)
    {
        var allowed = Transitions.TryGetValue(Status, out var targets) && Array.IndexOf(targets, next) >= 0;
        if (!allowed)
        {
            throw new ConflictException(nameof(Order), (long)Status, (long)next);
        }
        Status = next;
        Bump();
    }

    private void EnsureStatus(OrderStatus expected)
    {
        if (Status != expected)
        {
            throw new ValidationException("status", $"order is {Status}, expected {expected}");
        }
    }

    ~Order()
    {
        _lines.Clear();
    }
}

/// <summary>Called whenever stock of a SKU changes.</summary>
public delegate void StockChanged(Sku sku, int before, int after);

public delegate TResult Projection<in TSource, out TResult>(TSource source);

/// <summary>Small helpers over model collections.</summary>
public static class ModelExtensions
{
    public static int TotalAvailable(this IEnumerable<StockLine> lines, Sku sku) =>
        lines.Where(l => l.Sku == sku).Sum(l => l.Available);

    public static StockLine? Nearest(this IEnumerable<StockLine> lines, Location from, int needed)
    {
        return lines
            .Where(l => l.Available >= needed)
            .OrderBy(l => l.Bin.Distance(from))
            .ThenBy(l => l.Bin)
            .FirstOrDefault();
    }

    public static IEnumerable<TResult> Project<TSource, TResult>(
        this IEnumerable<TSource> source,
        Projection<TSource, TResult> projection)
    {
        foreach (var item in source)
        {
            yield return projection(item);
        }
    }

    public static string Describe(this Product product) => product switch
    {
        { Discontinued: true } => $"{product.Name} (discontinued)",
        { Price.Amount: 0m } => $"{product.Name} (free)",
        { Unit: Unit.Pallet or Unit.Box } p => $"{p.Name} per {p.Unit.ToString().ToLowerInvariant()}",
        _ => product.Name,
    };
}
