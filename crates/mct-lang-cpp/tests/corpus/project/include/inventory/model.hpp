// model.hpp — value types of the inventory system: identifiers, units,
// quantities, products and stock movements. Header-only.
#pragma once

#include <algorithm>
#include <array>
#include <chrono>
#include <cstdint>
#include <optional>
#include <ostream>
#include <string>
#include <string_view>
#include <variant>
#include <vector>

namespace inv {

/// Maximum length of a product name, in bytes (UTF-8).
constexpr std::size_t kMaxNameLength = 120;

/// Prefix every SKU starts with.
inline constexpr std::string_view kSkuPrefix = "SKU-";

using Clock = std::chrono::system_clock;
using Timestamp = Clock::time_point;
typedef std::uint32_t WarehouseId;
typedef std::vector<std::string> Tags;

/// Units a quantity can be expressed in.
enum class Unit : std::uint8_t {
    Piece,
    Kilogram,
    Litre,
    Metre,
};

/// Old-style unscoped enum, kept for the CSV importer.
enum LegacyFlag {
    kNone = 0,
    kFragile = 1 << 0,
    kHazardous = 1 << 1,
    kCold = 1 << 2,
};

/// Human-readable symbol of a unit.
constexpr const char* unit_symbol(Unit u) noexcept {
    switch (u) {
    case Unit::Piece:
        return "pc";
    case Unit::Kilogram:
        return "kg";
    case Unit::Litre:
        return "l";
    case Unit::Metre:
        return "m";
    }
    return "?";
}

/// Parses "pc"/"kg"/"l"/"m"; std::nullopt when unknown.
inline std::optional<Unit> parse_unit(std::string_view text) {
    if (text == "pc" || text == "piece") return Unit::Piece;
    if (text == "kg") return Unit::Kilogram;
    if (text == "l" || text == "L") return Unit::Litre;
    if (text == "m") return Unit::Metre;
    return std::nullopt;
}

/// Strongly-typed product identifier.
class ProductId {
public:
    constexpr ProductId() = default;
    constexpr explicit ProductId(std::uint64_t value) : value_(value) {}

    constexpr std::uint64_t value() const noexcept { return value_; }
    constexpr bool valid() const noexcept { return value_ != 0; }

    friend constexpr bool operator==(ProductId a, ProductId b) noexcept { return a.value_ == b.value_; }
    friend constexpr bool operator<(ProductId a, ProductId b) noexcept { return a.value_ < b.value_; }
    friend std::ostream& operator<<(std::ostream& os, ProductId id) { return os << "P" << id.value_; }

private:
    std::uint64_t value_ = 0;
};

/// Builds a SKU string from an id: "SKU-000042".
inline std::string make_sku(ProductId id) {
    std::string digits = std::to_string(id.value());
    if (digits.size() < 6) digits.insert(0, 6 - digits.size(), '0');
    return std::string(kSkuPrefix) + digits;
}

/// A non-negative amount in a given unit.
template <typename T = double>
class Quantity {
public:
    using value_type = T;

    Quantity() = default;
    Quantity(T amount, Unit unit) : amount_(clamp_non_negative(amount)), unit_(unit) {}

    T amount() const { return amount_; }
    Unit unit() const { return unit_; }

    Quantity operator+(const Quantity& other) const {
        require_same_unit(other);
        return Quantity(amount_ + other.amount_, unit_);
    }

    Quantity operator-(const Quantity& other) const {
        require_same_unit(other);
        return Quantity(amount_ - other.amount_, unit_);
    }

    Quantity& operator+=(const Quantity& other) {
        *this = *this + other;
        return *this;
    }

    bool operator<(const Quantity& other) const {
        require_same_unit(other);
        return amount_ < other.amount_;
    }

    bool is_zero() const { return amount_ == T{}; }

    template <typename U>
    Quantity<U> cast() const {
        return Quantity<U>(static_cast<U>(amount_), unit_);
    }

    std::string to_string() const { return std::to_string(amount_) + " " + unit_symbol(unit_); }

private:
    static T clamp_non_negative(T value) { return std::max(value, T{}); }

    void require_same_unit(const Quantity& other) const {
        if (other.unit_ != unit_) throw_unit_mismatch(unit_, other.unit_);
    }

    [[noreturn]] static void throw_unit_mismatch(Unit a, Unit b);

    T amount_{};
    Unit unit_ = Unit::Piece;
};

using Count = Quantity<std::int64_t>;
using Weight = Quantity<double>;

/// Physical size of a packed item, in centimetres.
struct Dimensions {
    double width = 0;
    double height = 0;
    double depth = 0;

    double volume() const { return width * height * depth; }
    bool fits_in(const Dimensions& box) const {
        std::array<double, 3> a{width, height, depth};
        std::array<double, 3> b{box.width, box.height, box.depth};
        std::sort(a.begin(), a.end());
        std::sort(b.begin(), b.end());
        return std::equal(a.begin(), a.end(), b.begin(), [](double x, double y) { return x <= y; });
    }
};

/// Where a product sits: warehouse, aisle, shelf.
struct Location {
    WarehouseId warehouse = 0;
    std::uint16_t aisle = 0;
    std::uint16_t shelf = 0;

    std::string label() const;
    auto operator<=>(const Location&) const = default;
};

/// A catalogue entry.
class Product {
public:
    /// Nested builder, so call sites read like named arguments.
    class Builder {
    public:
        explicit Builder(ProductId id) : id_(id) {}
        Builder& name(std::string value) {
            name_ = std::move(value);
            return *this;
        }
        Builder& unit(Unit value) {
            unit_ = value;
            return *this;
        }
        Builder& tag(std::string value) {
            tags_.push_back(std::move(value));
            return *this;
        }
        Builder& dimensions(Dimensions value) {
            dims_ = value;
            return *this;
        }
        Product build() const;

    private:
        ProductId id_;
        std::string name_;
        Unit unit_ = Unit::Piece;
        Tags tags_;
        std::optional<Dimensions> dims_;
    };

    Product(ProductId id, std::string name, Unit unit);
    virtual ~Product() = default;

    ProductId id() const { return id_; }
    const std::string& name() const { return name_; }
    Unit unit() const { return unit_; }
    const Tags& tags() const { return tags_; }
    bool has_tag(std::string_view tag) const;
    std::string sku() const { return make_sku(id_); }

    virtual std::string describe() const;
    virtual bool perishable() const { return false; }

protected:
    ProductId id_;
    std::string name_;
    Unit unit_;
    Tags tags_;
    std::optional<Dimensions> dims_;
    std::uint32_t flags_ = kNone;

    friend class Builder;
};

/// A product with a best-before date.
class PerishableProduct final : public Product {
public:
    PerishableProduct(ProductId id, std::string name, Unit unit, Timestamp best_before)
        : Product(id, std::move(name), unit), best_before_(best_before) {}

    bool perishable() const override { return true; }
    std::string describe() const override;
    bool expired(Timestamp now = Clock::now()) const { return now > best_before_; }

private:
    Timestamp best_before_;
};

/// Kinds of stock movement.
struct Receipt {
    Count amount;
    std::string supplier;
};
struct Shipment {
    Count amount;
    std::string order;
};
struct Adjustment {
    Count amount;
    bool increase = true;
    std::string reason;
};
using MovementKind = std::variant<Receipt, Shipment, Adjustment>;

/// One change to a product's stock level.
struct Movement {
    ProductId product;
    Location where;
    MovementKind kind;
    Timestamp at = Clock::now();

    std::int64_t delta() const;
    std::string_view kind_name() const;
};

/// Raw bytes or a parsed number, used by the barcode scanner integration.
union ScanValue {
    std::uint64_t numeric;
    char raw[8];
};

/// Visitor computing the signed stock change of a movement.
struct DeltaVisitor {
    std::int64_t operator()(const Receipt& r) const { return r.amount.amount(); }
    std::int64_t operator()(const Shipment& s) const { return -s.amount.amount(); }
    std::int64_t operator()(const Adjustment& a) const {
        return a.increase ? a.amount.amount() : -a.amount.amount();
    }
};

/// Class template argument deduction guide for a pair-like holder.
template <class A, class B>
struct Tagged {
    A tag;
    B value;
};
template <class A, class B>
Tagged(A, B) -> Tagged<A, B>;

inline std::int64_t Movement::delta() const {
    return std::visit(DeltaVisitor{}, kind);
}

inline std::string_view Movement::kind_name() const {
    switch (kind.index()) {
    case 0:
        return "receipt";
    case 1:
        return "shipment";
    default:
        return "adjustment";
    }
}

/// Sums a range of movements for one product.
template <typename It>
std::int64_t net_change(It first, It last, ProductId product) {
    std::int64_t total = 0;
    for (auto it = first; it != last; ++it) {
        if (it->product == product) total += it->delta();
    }
    return total;
}

/// Constrained helper: any container of Movement.
template <typename C>
    requires requires(const C& c) { c.begin(); c.end(); }
std::int64_t net_change(const C& movements, ProductId product) {
    return net_change(movements.begin(), movements.end(), product);
}

namespace detail {

/// Normalises a name: trims and collapses inner whitespace.
std::string normalize_name(std::string_view raw);

/// Validates a product name; returns an error message or empty.
std::string validate_name(std::string_view name);

constexpr int kSchemaVersion = 3;

}  // namespace detail

}  // namespace inv
