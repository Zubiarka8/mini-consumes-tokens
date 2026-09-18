using System;

namespace Omni.Payments
{
    public class PaymentProcessor
    {
        private readonly PaymentGateway gateway = new PaymentGateway();

        public int Process(int cents)
        {
            return this.gateway.Authorize(cents);
        }
    }
}
