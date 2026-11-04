<?php

namespace App\Http\Controllers;

use App\Billing\Customer as Payer;
use App\Events\OrderPlaced;
use App\Models\Order;

class OrderController extends Controller
{
    public function index()
    {
        return view('orders.index', ['orders' => Order::all()]);
    }

    public function store(Order $order, Payer $payer)
    {
        OrderPlaced::dispatch($order);
        return $payer->balance();
    }

    public function unused()
    {
        return null;
    }
}
