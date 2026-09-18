<?php

require_once 'Payable.php';

class Invoice implements Payable
{
    public function pay(float $amount): bool
    {
        return $this->validate($amount);
    }

    private function validate(float $amount): bool
    {
        return $amount > 0;
    }
}
