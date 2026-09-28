// Error types, result wrappers and guard helpers shared by every layer of
// the warehouse service. Nothing in here depends on the domain model, so
// Models.cs, Repository.cs and Services.cs can all reference it freely.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.CompilerServices;
using static System.Math;

namespace Warehouse.Errors
{
    /// <summary>Stable numeric codes, logged and returned by the API.</summary>
    public enum ErrorCode
    {
        None = 0,
        NotFound = 404,
        Conflict = 409,
        Validation = 422,
        OutOfStock = 460,
        QuotaExceeded = 429,
        Internal = 500,
    }

    [Flags]
    public enum Severity : byte
    {
        Info = 1,
        Warning = 2,
        Error = 4,
        Fatal = Error | 8,
    }

    /// <summary>Marks an exception type with its default code.</summary>
    [AttributeUsage(AttributeTargets.Class, Inherited = false)]
    public sealed class ErrorCodeAttribute : Attribute
    {
        public ErrorCodeAttribute(ErrorCode code) => Code = code;

        public ErrorCode Code { get; }

        public Severity Severity { get; init; } = Severity.Error;
    }

    /// <summary>Root of every domain exception.</summary>
    public abstract class WarehouseException : Exception
    {
        private readonly Dictionary<string, object?> _context = new();

        protected WarehouseException(string message, Exception? inner = null)
            : base(message, inner)
        {
            RaisedAt = DateTimeOffset.UtcNow;
        }

        public DateTimeOffset RaisedAt { get; }

        public abstract ErrorCode Code { get; }

        public IReadOnlyDictionary<string, object?> Context => _context;

        public WarehouseException With(string key, object? value)
        {
            _context[key] = value;
            return this;
        }

        public virtual string Describe()
        {
            var parts = new List<string> { $"[{(int)Code}] {Message}" };
            foreach (var (key, value) in _context)
            {
                parts.Add(string.Format(CultureInfo.InvariantCulture, "{0}={1}", key, value));
            }
            return string.Join("; ", parts);
        }

        public override string ToString() => Describe();
    }

    [ErrorCode(ErrorCode.NotFound, Severity = Severity.Warning)]
    public sealed class NotFoundException : WarehouseException
    {
        public NotFoundException(string entity, object key)
            : base($"{entity} '{key}' was not found")
        {
            Entity = entity;
            Key = key;
        }

        public string Entity { get; }

        public object Key { get; }

        public override ErrorCode Code => ErrorCode.NotFound;
    }

    [ErrorCode(ErrorCode.Validation)]
    public class ValidationException : WarehouseException
    {
        private readonly List<FieldError> _errors;

        public ValidationException(IEnumerable<FieldError> errors)
            : base("validation failed")
        {
            _errors = new List<FieldError>(errors);
        }

        public ValidationException(string field, string problem)
            : this(new[] { new FieldError(field, problem) })
        {
        }

        public IReadOnlyList<FieldError> Errors => _errors;

        public override ErrorCode Code => ErrorCode.Validation;

        public override string Describe()
        {
            var head = base.Describe();
            return _errors.Count == 0 ? head : head + ": " + string.Join(", ", _errors);
        }
    }

    public sealed class OutOfStockException : ValidationException
    {
        public OutOfStockException(string sku, int requested, int available)
            : base("quantity", $"{sku}: requested {requested}, only {available} left")
        {
            Shortfall = Max(0, requested - available);
        }

        public int Shortfall { get; }

        public override ErrorCode Code => ErrorCode.OutOfStock;
    }

    public sealed class ConflictException : WarehouseException
    {
        public ConflictException(string entity, long expected, long actual)
            : base($"{entity} changed: expected version {expected}, found {actual}")
        {
            Expected = expected;
            Actual = actual;
        }

        public long Expected { get; }

        public long Actual { get; }

        public override ErrorCode Code => ErrorCode.Conflict;
    }

    /// <summary>One invalid field and why.</summary>
    public readonly record struct FieldError(string Field, string Problem)
    {
        public override string ToString() => $"{Field}: {Problem}";
    }

    /// <summary>Either a value or an error, without throwing.</summary>
    public readonly struct Result<T>
    {
        private readonly T? _value;

        private Result(T? value, WarehouseException? error)
        {
            _value = value;
            Error = error;
        }

        public WarehouseException? Error { get; }

        public bool IsOk => Error is null;

        public T Value => IsOk ? _value! : throw Error!;

        public static Result<T> Ok(T value) => new(value, null);

        public static Result<T> Fail(WarehouseException error) => new(default, error);

        public Result<TOut> Map<TOut>(Func<T, TOut> map) =>
            IsOk ? Result<TOut>.Ok(map(_value!)) : Result<TOut>.Fail(Error!);

        public Result<TOut> Bind<TOut>(Func<T, Result<TOut>> next) =>
            IsOk ? next(_value!) : Result<TOut>.Fail(Error!);

        public T OrElse(T fallback) => IsOk ? _value! : fallback;

        public static implicit operator Result<T>(T value) => Ok(value);

        public void Deconstruct(out bool ok, out T? value, out WarehouseException? error)
        {
            ok = IsOk;
            value = _value;
            error = Error;
        }
    }

    /// <summary>Precondition checks that throw domain exceptions.</summary>
    public static class Guard
    {
        public static string NotBlank(
            string? value,
            [CallerArgumentExpression(nameof(value))] string field = "")
        {
            if (string.IsNullOrWhiteSpace(value))
            {
                throw new ValidationException(field, "must not be blank");
            }
            return value.Trim();
        }

        public static int Positive(int value, string field)
        {
            return value > 0 ? value : throw new ValidationException(field, "must be positive");
        }

        public static T Found<T>(T? value, string entity, object key)
            where T : class
        {
            return value ?? throw new NotFoundException(entity, key);
        }

        public static void Version(string entity, long expected, long actual)
        {
            if (expected != actual)
            {
                throw new ConflictException(entity, expected, actual);
            }
        }

        public static decimal InRange(this decimal value, decimal low, decimal high, string field)
        {
            if (value < low || value > high)
            {
                throw new ValidationException(field, $"must be between {low} and {high}");
            }
            return Round(value, 2);
        }
    }

    /// <summary>Collects validation problems before failing once.</summary>
    public sealed class ErrorCollector
    {
        private readonly List<FieldError> _errors = new();

        public bool HasErrors => _errors.Count > 0;

        public ErrorCollector Check(bool condition, string field, string problem)
        {
            if (!condition)
            {
                _errors.Add(new FieldError(field, problem));
            }
            return this;
        }

        public ErrorCollector Require(string? value, string field) =>
            Check(!string.IsNullOrWhiteSpace(value), field, "is required");

        public void ThrowIfAny()
        {
            if (HasErrors)
            {
                throw new ValidationException(_errors);
            }
        }
    }

    /// <summary>Human-readable messages per code, with nested lookup types.</summary>
    public static class ErrorCatalog
    {
        private static readonly Dictionary<ErrorCode, Entry> Entries = Build();

        public sealed record Entry(ErrorCode Code, string Title, Severity Severity)
        {
            public string Render(string detail) => $"{Title} ({(int)Code}): {detail}";
        }

        public static class Http
        {
            public static int StatusFor(ErrorCode code) => code switch
            {
                ErrorCode.None => 200,
                ErrorCode.OutOfStock or ErrorCode.Validation => 422,
                ErrorCode.QuotaExceeded => 429,
                var c when (int)c >= 500 => 500,
                _ => (int)code,
            };
        }

        private static Dictionary<ErrorCode, Entry> Build()
        {
            var entries = new Dictionary<ErrorCode, Entry>();
            void Add(ErrorCode code, string title, Severity severity) =>
                entries[code] = new Entry(code, title, severity);

            Add(ErrorCode.NotFound, "Not found", Severity.Warning);
            Add(ErrorCode.Conflict, "Edit conflict", Severity.Warning);
            Add(ErrorCode.Validation, "Invalid input", Severity.Info);
            Add(ErrorCode.OutOfStock, "Out of stock", Severity.Warning);
            Add(ErrorCode.QuotaExceeded, "Slow down", Severity.Info);
            Add(ErrorCode.Internal, "Internal error", Severity.Fatal);
            return entries;
        }

        public static string Explain(WarehouseException error)
        {
            if (!Entries.TryGetValue(error.Code, out var entry))
            {
                return error.Describe();
            }
            return entry.Render(error.Message);
        }

        public static Severity SeverityOf(Exception error) => error switch
        {
            WarehouseException { Code: var code } when Entries.ContainsKey(code) => Entries[code].Severity,
            OperationCanceledException => Severity.Info,
            _ => Severity.Fatal,
        };
    }

    /// <summary>Token bucket used to throttle noisy callers.</summary>
    public sealed class RateLimiter
    {
        private readonly object _gate = new();
        private readonly int _capacity;
        private readonly TimeSpan _refill;
        private double _tokens;
        private DateTime _last;

        public RateLimiter(int capacity, TimeSpan refill)
        {
            _capacity = Guard.Positive(capacity, nameof(capacity));
            _refill = refill;
            _tokens = capacity;
            _last = DateTime.UtcNow;
        }

        public bool TryAcquire(int cost = 1)
        {
            lock (_gate)
            {
                Refill();
                if (_tokens < cost)
                {
                    return false;
                }
                _tokens -= cost;
                return true;
            }
        }

        public void Acquire(int cost = 1)
        {
            if (!TryAcquire(cost))
            {
                throw new QuotaExceededException(_capacity);
            }
        }

        private void Refill()
        {
            var now = DateTime.UtcNow;
            var earned = (now - _last).TotalMilliseconds / Max(1, _refill.TotalMilliseconds);
            _tokens = Min(_capacity, _tokens + earned * _capacity);
            _last = now;
        }

        private sealed class QuotaExceededException : WarehouseException
        {
            public QuotaExceededException(int capacity)
                : base($"more than {capacity} requests per window") { }

            public override ErrorCode Code => ErrorCode.QuotaExceeded;
        }
    }
}
