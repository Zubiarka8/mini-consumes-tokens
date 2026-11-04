<?php

namespace App\Events;

use App\Models\Order;

class OrderPlaced
{
    public function __construct(public Order $order)
    {
    }
}
