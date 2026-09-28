// service.cpp — business rules: receiving, shipping, transfers, reservations
// and reorder suggestions, built on the repositories of storage.hpp.

#include "inventory/model.hpp"
#include "inventory/storage.hpp"

#include <cmath>
#include <iostream>
#include <numeric>
#include <set>
#include <utility>

namespace inv::service {

inline namespace v2 {

/// Result of an operation that may fail without an exception.
template <typename T>
class Result {
public:
    static Result ok(T value) { return Result(std::move(value), {}); }
    static Result fail(std::string error) { return Result(std::nullopt, std::move(error)); }

    bool has_value() const { return value_.has_value(); }
    const T& value() const { return *value_; }
    const std::string& error() const { return error_; }

    template <typename F>
    auto map(F&& fn) const -> Result<decltype(fn(std::declval<const T&>()))> {
        using U = decltype(fn(std::declval<const T&>()));
        if (!has_value()) return Result<U>::fail(error_);
        return Result<U>::ok(fn(*value_));
    }

private:
    Result(std::optional<T> value, std::string error) : value_(std::move(value)), error_(std::move(error)) {}

    std::optional<T> value_;
    std::string error_;
};

/// CRTP base giving every listener a stable name and a counter.
template <typename Derived>
class CountingListener {
public:
    void notify(const Movement& movement) {
        ++seen_;
        static_cast<Derived*>(this)->on_movement(movement);
    }
    std::size_t seen() const { return seen_; }

private:
    std::size_t seen_ = 0;
};

/// Logs every movement to a stream.
class LoggingListener : public CountingListener<LoggingListener> {
public:
    explicit LoggingListener(std::ostream& out) : out_(out) {}
    void on_movement(const Movement& movement) {
        out_ << movement.kind_name() << " " << movement.product << " " << movement.delta() << '\n';
    }

private:
    std::ostream& out_;
};

/// Reorder policy: minimum level and batch size per product.
struct ReorderRule {
    std::int64_t minimum = 0;
    std::int64_t batch = 1;
};

/// A suggested purchase order line.
struct Suggestion {
    ProductId product;
    std::int64_t quantity;
    std::string reason;
};

class InventoryService {
public:
    using Observer = std::function<void(const Movement&)>;

    InventoryService(ProductRepository& products, StockRepository& stock, FileStore* journal = nullptr);

    Result<StockLevel> receive(ProductId product, Location where, std::int64_t amount, std::string supplier);
    Result<StockLevel> ship(ProductId product, Location where, std::int64_t amount, std::string order);
    Result<std::pair<StockLevel, StockLevel>> transfer(ProductId product, Location from, Location to,
                                                       std::int64_t amount);
    bool reserve(ProductId product, Location where, std::int64_t amount);
    void release(ProductId product, Location where, std::int64_t amount);

    std::int64_t total_on_hand(ProductId product) const;
    std::vector<Suggestion> reorder_suggestions() const;

    void set_rule(ProductId product, ReorderRule rule) { rules_[product] = rule; }
    void subscribe(Observer observer) { observers_.push_back(std::move(observer)); }

    static std::size_t instances() { return instance_count_; }

private:
    StockLevel level_at(ProductId product, Location where) const;
    void record(Movement movement);
    void check_product(ProductId product) const;

    ProductRepository& products_;
    StockRepository& stock_;
    FileStore* journal_;
    std::map<ProductId, ReorderRule> rules_;
    std::vector<Observer> observers_;

    static inline std::size_t instance_count_ = 0;
};

InventoryService::InventoryService(ProductRepository& products, StockRepository& stock, FileStore* journal)
    : products_(products), stock_(stock), journal_(journal) {
    ++instance_count_;
}

void InventoryService::check_product(ProductId product) const {
    if (!product.valid()) throw std::invalid_argument("invalid product id");
    products_.require(product);
}

StockLevel InventoryService::level_at(ProductId product, Location where) const {
    const auto found = stock_.find(StockKey{product, where});
    if (found) return *found;
    return StockLevel{product, where, Count(0, Unit::Piece), Count(0, Unit::Piece)};
}

void InventoryService::record(Movement movement) {
    if (journal_ != nullptr) journal_->append(movement);
    for (const auto& observer : observers_) observer(movement);
}

Result<StockLevel> InventoryService::receive(ProductId product, Location where, std::int64_t amount,
                                             std::string supplier) {
    if (amount <= 0) return Result<StockLevel>::fail("receive amount must be positive");
    check_product(product);
    auto level = level_at(product, where);
    level.on_hand += Count(amount, Unit::Piece);
    stock_.put(StockKey{product, where}, level);
    record(Movement{product, where, Receipt{Count(amount, Unit::Piece), std::move(supplier)}});
    return Result<StockLevel>::ok(level);
}

Result<StockLevel> InventoryService::ship(ProductId product, Location where, std::int64_t amount,
                                          std::string order) {
    check_product(product);
    auto level = level_at(product, where);
    if (level.available().amount() < amount) {
        return Result<StockLevel>::fail("insufficient stock at " + where.label() + ": " +
                                        std::to_string(level.available().amount()) + " < " +
                                        std::to_string(amount));
    }
    level.on_hand = level.on_hand - Count(amount, Unit::Piece);
    stock_.put(StockKey{product, where}, level);
    record(Movement{product, where, Shipment{Count(amount, Unit::Piece), std::move(order)}});
    return Result<StockLevel>::ok(level);
}

Result<std::pair<StockLevel, StockLevel>> InventoryService::transfer(ProductId product, Location from,
                                                                     Location to, std::int64_t amount) {
    using Pair = std::pair<StockLevel, StockLevel>;
    if (from == to) return Result<Pair>::fail("transfer to the same location");
    Transaction tx(products_, stock_);
    auto shipped = ship(product, from, amount, "transfer to " + to.label());
    if (!shipped.has_value()) return Result<Pair>::fail(shipped.error());
    auto received = receive(product, to, amount, "transfer from " + from.label());
    if (!received.has_value()) return Result<Pair>::fail(received.error());
    tx.commit();
    return Result<Pair>::ok({shipped.value(), received.value()});
}

bool InventoryService::reserve(ProductId product, Location where, std::int64_t amount) {
    auto level = level_at(product, where);
    if (level.available().amount() < amount) return false;
    level.reserved += Count(amount, Unit::Piece);
    stock_.put(StockKey{product, where}, level);
    return true;
}

void InventoryService::release(ProductId product, Location where, std::int64_t amount) {
    auto level = level_at(product, where);
    const auto released = std::min(amount, level.reserved.amount());
    level.reserved = level.reserved - Count(released, Unit::Piece);
    stock_.put(StockKey{product, where}, level);
}

std::int64_t InventoryService::total_on_hand(ProductId product) const {
    std::int64_t total = 0;
    stock_.for_each([&](const StockKey& key, const StockLevel& level) {
        if (key.product == product) total += level.on_hand.amount();
    });
    return total;
}

std::vector<Suggestion> InventoryService::reorder_suggestions() const {
    std::vector<Suggestion> out;
    for (const auto& [product, rule] : rules_) {
        const auto on_hand = total_on_hand(product);
        if (on_hand >= rule.minimum) continue;
        const auto missing = rule.minimum - on_hand;
        const auto batches = (missing + rule.batch - 1) / rule.batch;
        out.push_back({product, batches * rule.batch, "below minimum " + std::to_string(rule.minimum)});
    }
    std::sort(out.begin(), out.end(), [](const Suggestion& a, const Suggestion& b) {
        return a.quantity > b.quantity;
    });
    return out;
}

}  // namespace v2

// ---------------------------------------------------------------------------
// Free helpers of the service namespace
// ---------------------------------------------------------------------------

/// Average stock per location for a product, rounded to 2 decimals.
double average_stock(const InventoryService& service, ProductId product, std::size_t locations) {
    if (locations == 0) return 0.0;
    const double avg = static_cast<double>(service.total_on_hand(product)) / static_cast<double>(locations);
    return std::round(avg * 100.0) / 100.0;
}

/// Distinct warehouses holding a product.
std::set<WarehouseId> warehouses_of(const StockRepository& stock, ProductId product) {
    std::set<WarehouseId> out;
    stock.for_each([&out, product](const StockKey& key, const StockLevel&) {
        if (key.product == product) out.insert(key.location.warehouse);
    });
    return out;
}

/// Total value of stock given a unit-price table.
double stock_value(const StockRepository& stock, const std::map<ProductId, double>& prices) {
    const auto levels = stock.values();
    return std::accumulate(levels.begin(), levels.end(), 0.0, [&prices](double acc, const StockLevel& level) {
        const auto it = prices.find(level.product);
        return it == prices.end() ? acc : acc + it->second * static_cast<double>(level.on_hand.amount());
    });
}

/// Seeds a service with demo data; used by `inventory demo`.
void seed_demo(InventoryService& service, ProductRepository& products) {
    const auto hammer = Product::Builder(ProductId(1)).name("Hammer").tag("tools").build();
    const auto flour = Product::Builder(ProductId(2)).name("Harina de trigo").unit(Unit::Kilogram).build();
    const auto rope = Product::Builder(ProductId(3))
                          .name("Rope")
                          .unit(Unit::Metre)
                          .tag("outdoor")
                          .dimensions({10, 10, 30})
                          .build();
    for (const auto& product : {hammer, flour, rope}) products.put(product.id(), product);
    const Location main_hall{1, 1, 1};
    const Location cold_room{1, 9, 2};
    service.receive(hammer.id(), main_hall, 40, "Acme");
    service.receive(flour.id(), cold_room, 250, "Molinos del Sur");
    service.receive(rope.id(), main_hall, 500, "Cordelería");
    service.set_rule(hammer.id(), ReorderRule{50, 25});
    service.set_rule(flour.id(), {100, 50});
}

/// Wires a logging observer into the service; returns the listener so the
/// caller controls its lifetime.
std::unique_ptr<LoggingListener> attach_logging(InventoryService& service, std::ostream& out) {
    auto listener = std::make_unique<LoggingListener>(out);
    service.subscribe([ptr = listener.get()](const Movement& movement) { ptr->notify(movement); });
    return listener;
}

/// Abstract pricing strategy with two implementations.
class PricingStrategy {
public:
    virtual ~PricingStrategy() = default;
    virtual double price(const Product& product, std::int64_t quantity) const = 0;
};

class FlatPricing final : public PricingStrategy {
public:
    explicit FlatPricing(double unit) : unit_(unit) {}
    double price(const Product&, std::int64_t quantity) const override { return unit_ * quantity; }

private:
    double unit_;
};

/// Volume discount: every full `step` units lowers the unit price by `rate`.
double tiered_price(double unit, std::int64_t quantity, std::int64_t step, double rate) {
    struct Tier {
        std::int64_t from;
        double unit;
    };
    const auto tiers = [&] {
        std::vector<Tier> out;
        for (std::int64_t i = 0; i * step <= quantity && i < 5; ++i) out.push_back({i * step, unit * (1 - rate * i)});
        return out;
    }();
    return quantity * tiers.back().unit;
}

/// Factory used by main.cpp.
std::unique_ptr<InventoryService> make_service(ProductRepository& products, StockRepository& stock,
                                               FileStore* journal) {
    return std::make_unique<InventoryService>(products, stock, journal);
}

}  // namespace inv::service
