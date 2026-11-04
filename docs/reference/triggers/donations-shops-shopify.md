# Donations & Shops / Shopify (triggers)

Shop order events that Shopify sends to Streamerz's HTTP server.

## Shopify Order Created

- Kind: trigger
- Code: 13001
- Type: ShopifyOrderCreated
- Category: Donations & Shops/Shopify
- Needs: the HTTP server running (off by default, port 7474), and Shopify's webhook sent to it

Fires when a Shopify order is created.

### Fields

None.

### Arguments

Streamer.bot's names: the order flattened under `shopify.` (`shopify.total_price`,
`shopify.line_items[0].title`...) and `shopify.event` (the topic). The body's own top-level keys are kept
too.

### When

When Shopify posts to `/shopify` (or `/api/shopify`) with a `topic` other than `orders/paid` or `OrderPaid`. A body with no `topic` counts as an order created.

### Notes

- Streamerz reads the topic from a `topic` key in the JSON body. The topic header Shopify sends isn't read.

## Shopify Order Paid

- Kind: trigger
- Code: 13002
- Type: ShopifyOrderPaid
- Category: Donations & Shops/Shopify
- Needs: the HTTP server running (off by default, port 7474), and Shopify's webhook sent to it

Fires when a Shopify order is paid.

### Fields

None.

### Arguments

Streamer.bot's names: the order flattened under `shopify.` (`shopify.total_price`,
`shopify.line_items[0].title`...) and `shopify.event` (the topic). The body's own top-level keys are kept
too.

### When

When Shopify posts to `/shopify` (or `/api/shopify`) with a `topic` of `orders/paid` or `OrderPaid`.

### Notes

- Streamerz reads the topic from a `topic` key in the JSON body. The topic header Shopify sends isn't read.
