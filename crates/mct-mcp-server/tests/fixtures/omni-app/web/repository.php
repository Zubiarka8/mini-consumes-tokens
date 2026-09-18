<?php

function load_rows($account)
{
    return [$account => 0];
}

class RowStore
{
    public function fetch($account)
    {
        return load_rows($account);
    }
}
