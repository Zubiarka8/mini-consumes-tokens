// Persistence: a generic repository contract, an in-memory implementation
// with optimistic concurrency, change notifications and a unit of work that
// commits several repositories together.

global using System.Threading.Tasks;

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using Warehouse.Errors;
using Warehouse.Models;

namespace Warehouse.Data
{
    /// <summary>What happened to an entity.</summary>
    public enum ChangeKind
    {
        Added,
        Updated,
        Removed,
    }

    public sealed class ChangeEventArgs<TKey> : EventArgs
    {
        public ChangeEventArgs(TKey key, ChangeKind kind, long version)
        {
            Key = key;
            Kind = kind;
            Version = version;
        }

        public TKey Key { get; }

        public ChangeKind Kind { get; }

        public long Version { get; }
    }

    /// <summary>Read side of a repository.</summary>
    public interface IReadRepository<TKey, TEntity>
        where TKey : notnull
        where TEntity : class, IEntity<TKey>
    {
        Task<TEntity?> FindAsync(TKey key, CancellationToken token = default);

        Task<IReadOnlyList<TEntity>> ListAsync(Func<TEntity, bool>? filter = null, CancellationToken token = default);

        async Task<TEntity> GetAsync(TKey key, CancellationToken token = default)
        {
            var found = await FindAsync(key, token).ConfigureAwait(false);
            return Guard.Found(found, typeof(TEntity).Name, key);
        }
    }

    /// <summary>Full repository: reads plus versioned writes.</summary>
    public interface IRepository<TKey, TEntity> : IReadRepository<TKey, TEntity>
        where TKey : notnull
        where TEntity : class, IEntity<TKey>
    {
        event EventHandler<ChangeEventArgs<TKey>>? Changed;

        Task SaveAsync(TEntity entity, long expectedVersion, CancellationToken token = default);

        Task<bool> RemoveAsync(TKey key, CancellationToken token = default);
    }

    /// <summary>Thread-safe in-memory store; the default in tests and demos.</summary>
    public class InMemoryRepository<TKey, TEntity> : IRepository<TKey, TEntity>, IDisposable
        where TKey : notnull
        where TEntity : class, IEntity<TKey>
    {
        private static long s_instances;

        private readonly ConcurrentDictionary<TKey, TEntity> _items = new();
        private readonly ReaderWriterLockSlim _lock = new();
        private EventHandler<ChangeEventArgs<TKey>>? _changed;
        private bool _disposed;

        static InMemoryRepository()
        {
            s_instances = 0;
        }

        public InMemoryRepository()
        {
            Interlocked.Increment(ref s_instances);
        }

        public InMemoryRepository(IEnumerable<TEntity> seed)
            : this()
        {
            foreach (var entity in seed)
            {
                _items[entity.Id] = entity;
            }
        }

        public static long Instances => Interlocked.Read(ref s_instances);

        public int Count => _items.Count;

        public TEntity this[TKey key] => _items.TryGetValue(key, out var e) ? e : throw new NotFoundException(typeof(TEntity).Name, key);

        public event EventHandler<ChangeEventArgs<TKey>>? Changed
        {
            add => _changed += value;
            remove => _changed -= value;
        }

        public Task<TEntity?> FindAsync(TKey key, CancellationToken token = default)
        {
            token.ThrowIfCancellationRequested();
            return Task.FromResult(_items.TryGetValue(key, out var entity) ? entity : null);
        }

        public Task<IReadOnlyList<TEntity>> ListAsync(Func<TEntity, bool>? filter = null, CancellationToken token = default)
        {
            token.ThrowIfCancellationRequested();
            IReadOnlyList<TEntity> result = Read(() => _items.Values.Where(filter ?? (_ => true)).ToList());
            return Task.FromResult(result);
        }

        public virtual Task SaveAsync(TEntity entity, long expectedVersion, CancellationToken token = default)
        {
            token.ThrowIfCancellationRequested();
            var kind = Write(() =>
            {
                if (_items.TryGetValue(entity.Id, out var current))
                {
                    Guard.Version(typeof(TEntity).Name, expectedVersion, current.Version - (ReferenceEquals(current, entity) ? 1 : 0));
                    _items[entity.Id] = entity;
                    return ChangeKind.Updated;
                }
                _items[entity.Id] = entity;
                return ChangeKind.Added;
            });
            OnChanged(entity.Id, kind, entity.Version);
            return Task.CompletedTask;
        }

        public Task<bool> RemoveAsync(TKey key, CancellationToken token = default)
        {
            token.ThrowIfCancellationRequested();
            var removed = Write(() => _items.TryRemove(key, out _));
            if (removed)
            {
                OnChanged(key, ChangeKind.Removed, -1);
            }
            return Task.FromResult(removed);
        }

        protected virtual void OnChanged(TKey key, ChangeKind kind, long version)
        {
            _changed?.Invoke(this, new ChangeEventArgs<TKey>(key, kind, version));
        }

        private T Read<T>(Func<T> body)
        {
            _lock.EnterReadLock();
            try
            {
                return body();
            }
            finally
            {
                _lock.ExitReadLock();
            }
        }

        private T Write<T>(Func<T> body)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            _lock.EnterWriteLock();
            try
            {
                return body();
            }
            finally
            {
                _lock.ExitWriteLock();
            }
        }

        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
            _lock.Dispose();
            GC.SuppressFinalize(this);
        }
    }

    namespace Specialized
    {
        /// <summary>Products, with lookups by tag and family.</summary>
        public sealed class ProductRepository : InMemoryRepository<Sku, Product>
        {
            public ProductRepository(IEnumerable<Product> seed)
                : base(seed)
            {
            }

            public async Task<IReadOnlyList<Product>> ByTagAsync(string tag, CancellationToken token = default)
            {
                var all = await ListAsync(p => p.Tags.Contains(tag), token);
                return all.OrderBy(p => p.Name, StringComparer.Ordinal).ToList();
            }

            public async Task<ILookup<string, Product>> ByFamilyAsync(CancellationToken token = default)
            {
                var all = await ListAsync(null, token);
                return all.ToLookup(p => p.Id.Family);
            }
        }

        /// <summary>Stock lines keyed by (SKU, bin).</summary>
        public sealed class StockRepository : InMemoryRepository<(Sku, Location), StockLine>
        {
            public StockRepository()
                : base()
            {
            }

            public async Task<IReadOnlyList<StockLine>> ForSkuAsync(Sku sku, CancellationToken token = default)
            {
                return await ListAsync(line => line.Sku == sku, token);
            }

            public async Task<int> AvailableAsync(Sku sku, CancellationToken token = default)
            {
                var lines = await ForSkuAsync(sku, token);
                return lines.TotalAvailable(sku);
            }

            protected override void OnChanged((Sku, Location) key, ChangeKind kind, long version)
            {
                base.OnChanged(key, kind, version);
                LastChanged = key.Item1;
            }

            public Sku? LastChanged { get; private set; }
        }

        /// <summary>Orders keyed by id.</summary>
        public sealed class OrderRepository : InMemoryRepository<Guid, Order>
        {
            public Task<IReadOnlyList<Order>> OpenAsync(CancellationToken token = default) =>
                ListAsync(o => o.Status is OrderStatus.Placed or OrderStatus.Picking, token);

            public async IAsyncEnumerable<Order> StreamAsync(string customer)
            {
                foreach (var order in await ListAsync(o => o.Customer == customer))
                {
                    await Task.Yield();
                    yield return order;
                }
            }
        }
    }

    /// <summary>Commits pending writes to several repositories at once.</summary>
    public sealed class UnitOfWork : IAsyncDisposable
    {
        private readonly List<Func<CancellationToken, Task>> _pending = new();
        private readonly RateLimiter _limiter;
        private int _commits;

        public UnitOfWork(RateLimiter limiter) => _limiter = limiter;

        public int Pending => _pending.Count;

        public int Commits => _commits;

        public UnitOfWork Save<TKey, TEntity>(IRepository<TKey, TEntity> repository, TEntity entity)
            where TKey : notnull
            where TEntity : class, IEntity<TKey>
        {
            var expected = entity.Version;
            _pending.Add(token => repository.SaveAsync(entity, expected, token));
            return this;
        }

        public UnitOfWork Remove<TKey, TEntity>(IRepository<TKey, TEntity> repository, TKey key)
            where TKey : notnull
            where TEntity : class, IEntity<TKey>
        {
            _pending.Add(async token => await repository.RemoveAsync(key, token));
            return this;
        }

        public async Task CommitAsync(CancellationToken token = default)
        {
            _limiter.Acquire(Math.Max(1, _pending.Count));
            var batch = _pending.ToArray();
            _pending.Clear();
            foreach (var step in batch)
            {
                await step(token).ConfigureAwait(false);
            }
            Interlocked.Increment(ref _commits);
        }

        public ValueTask DisposeAsync()
        {
            _pending.Clear();
            return ValueTask.CompletedTask;
        }
    }

    /// <summary>Loads demo data into fresh repositories.</summary>
    public static class Seed
    {
        public static (Specialized.ProductRepository Products, Specialized.StockRepository Stock) Demo()
        {
            var products = new[]
            {
                new Product(Sku.Parse("BOL-0001"), "Bolt M6", new Price(0.12m), Unit.Box).Tag("hardware", "metric"),
                new Product(Sku.Parse("NUT-0006"), "Nut M6", new Price(0.05m), Unit.Box).Tag("hardware"),
                new Product(Sku.Parse("PAL-0100"), "Euro pallet", new Price(9.50m), Unit.Pallet),
                new Product(Sku.Parse("TAP-0042"), "Packing tape", new Price(2.30m)).Tag("packing"),
            };
            var stock = new Specialized.StockRepository();
            foreach (var product in products)
            {
                var line = new StockLine(product.Id, Location.Parse("A-01-1"), 100);
                stock.SaveAsync(line, 0).GetAwaiter().GetResult();
            }
            return (new Specialized.ProductRepository(products), stock);
        }
    }
}
