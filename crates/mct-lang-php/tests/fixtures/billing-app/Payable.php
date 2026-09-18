<?php

interface Payable
{
    public function pay(float $amount): bool;
}
