package com.acme.app;

import static com.acme.util.Strings.trim;

import com.acme.model.Money;
import com.acme.model.*;
import java.util.List;

public class Main {
    public static void main(String[] args) {
        List<Money> all = List.of(new Money(trim(" 1 ")));
    }
}
