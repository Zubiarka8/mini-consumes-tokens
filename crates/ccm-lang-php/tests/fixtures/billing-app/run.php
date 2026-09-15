<?php

require_once 'Invoice.php';

function main(): void
{
    $invoice = new Invoice();
    $invoice->pay(10.0);
}

main();
