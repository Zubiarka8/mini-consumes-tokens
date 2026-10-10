"use client";

import { useState } from "react";
import { useTranslation } from "react-i18next";

const source = `const currency = "EUR";
const taxRate = 0.21;
const paymentTerms = 30;

export function calculateTotal(price, quantity = 1) {
  const subtotal = price * quantity;
  const tax = subtotal * taxRate;
  return subtotal + tax;
}

export function formatPrice(price) {
  return new Intl.NumberFormat("en-IE", {
    style: "currency",
    currency,
  }).format(price);
}

export function invoiceLabel(id) {
  return \`Invoice #\${id}\`;
}

export function paymentDue(issuedAt) {
  const due = new Date(issuedAt);
  due.setDate(due.getDate() + paymentTerms);
  return due.toISOString().slice(0, 10);
}

export function customerDetails(customer) {
  return {
    name: customer.name,
    email: customer.email,
    address: customer.billingAddress,
  };
}

export function invoiceStatus(invoice) {
  if (invoice.paidAt) return "paid";
  const due = paymentDue(invoice.issuedAt);
  const today = new Date().toISOString().slice(0, 10);
  return due < today ? "overdue" : "pending";
}

export function createInvoice(id, customer, items) {
  return {
    id,
    customer: customerDetails(customer),
    items,
    issuedAt: new Date().toISOString(),
    paidAt: null,
  };
}`;

const lines = source.split("\n").map((text, index) => ({
  text,
  number: index + 1,
  relevant: index === 1 || (index >= 4 && index <= 8),
}));
const selection = lines.filter((line) => line.relevant);

export function ContextDemo() {
  const { t } = useTranslation();
  const [focused, setFocused] = useState(false);
  const visible = focused ? selection : lines;

  return (
    <section className="context-demo" aria-labelledby="demo-title">
      <div className="demo-intro">
        <span className="demo-badge">{t("demoBadge")}</span>
        <h2 id="demo-title">{t("demoTitle")}</h2>
        <p>{t("demoQuery")}</p>
        <div className="demo-count">
          <strong>{lines.length}</strong>
          <span aria-hidden="true">→</span>
          <strong>{selection.length}</strong>
          <span>{t("demoLines")}</span>
        </div>
        <div className="demo-controls" role="group" aria-label={t("demoView")}>
          {[false, true].map((mode) => (
            <button
              key={String(mode)}
              type="button"
              aria-pressed={focused === mode}
              aria-controls="demo-code"
              onClick={() => setFocused(mode)}
            >
              <span aria-hidden="true">{focused === mode ? "●" : "○"}</span>
              {t(mode ? "demoRelevant" : "demoFull")}
            </button>
          ))}
        </div>
      </div>
      <div className="demo-filebar">
        <span>invoice.js</span>
        <span>{t(focused ? "demoRelevant" : "demoFull")}</span>
      </div>
      <div className="demo-editor">
        <div
          id="demo-code"
          className="demo-code"
          tabIndex={0}
          role="region"
          aria-label={t("demoCode")}
          key={String(focused)}
        >
          <pre>
            <code>
              {visible.map((line) => (
                <span
                  key={line.number}
                  className={`demo-line${line.relevant ? " is-relevant" : ""}`}
                >
                  <span className="demo-line-number" aria-hidden="true">
                    {line.number}
                  </span>
                  <span>{line.text || " "}</span>
                  {"\n"}
                </span>
              ))}
            </code>
          </pre>
        </div>
        <div className="demo-minimap" aria-hidden="true">
          {lines.map((line) => (
            <span
              key={line.number}
              className={
                line.relevant ? "is-relevant" : focused ? "is-omitted" : ""
              }
            />
          ))}
        </div>
      </div>
      <div className="demo-summary">
        <p role="status">{t(focused ? "demoSelectionNote" : "demoFullNote")}</p>
      </div>
      <p className="demo-disclaimer">{t("demoDisclaimer")}</p>
    </section>
  );
}
