<?php

namespace App\Listeners;

use App\Events\OrderPlaced;

class NotifyWarehouse
{
    public function handle(OrderPlaced $event): void
    {
    }
}
