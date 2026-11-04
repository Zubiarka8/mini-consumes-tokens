<?php

namespace App\Listeners;

use App\Events\OrderPlaced;

class AuditOrder
{
    public function handle(OrderPlaced $event): void
    {
    }
}
