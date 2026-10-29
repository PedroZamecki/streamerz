# Donations & Shops / Streamlabs (triggers)

Donations, charity donations and merchandise sent through Streamlabs, and the state of its connection.

Streamlabs events reach Streamerz in two ways: over the Streamlabs socket (once a socket API token is set in Donations) and through the local webhook (`/api/streamlabs`). Both start the same triggers.

## Donation

- Kind: trigger
- Code: 201
- Type: StreamlabsDonation
- Category: Donations & Shops/Streamlabs
- Needs: a Streamlabs socket API token in Donations, or Streamlabs events posted to the webhook

Starts its actions for each donation from Streamlabs.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Min amount (0: none) | `min` | The least amount to fire for. 0: no minimum. |
| Max amount (0: none) | `max` | With Min at 0 or empty, fires only for exactly this amount. With Min set, the top of the range. 0: no maximum. |

### Arguments

- `donationFrom`: who donated.
- `donationAmount`: the amount, as a number.
- `donationCurrency`: the currency.
- `donationFormattedAmount`: the amount as Streamlabs formats it.
- `donationMessage`: the message with the donation.

### When

When Streamlabs sends a donation, over the socket or to the webhook. A socket message that holds several donations starts the trigger once for each. Only donations inside the amount range set on the trigger start it.

### Notes

- With neither Min nor Max set, every donation starts the trigger.

## Merchandise

- Kind: trigger
- Code: 202
- Type: StreamlabsMerchandise
- Category: Donations & Shops/Streamlabs
- Needs: a Streamlabs socket API token in Donations, or Streamlabs events posted to the webhook

Starts its actions for each merchandise purchase from Streamlabs.

### Fields

None.

### Arguments

- `merchandiseFrom`: who bought the item.
- `merchandiseMessage`: the message with the purchase.
- `merchandiseProduct`: the product bought.
- `merchandiseImageUrl`: the product's picture.
- `merchandiseImageEscaped`: the picture's address, URL encoded.

### When

When Streamlabs sends a merchandise purchase, over the socket or to the webhook.

## Connected

- Kind: trigger
- Code: 203
- Type: StreamlabsConnected
- Category: Donations & Shops/Streamlabs
- Needs: a Streamlabs socket API token in Donations

Starts its actions when the Streamlabs socket connects.

### Fields

None.

### Arguments

None set.

### When

When the Streamlabs socket connects and Streamlabs accepts the token. A Streamlabs event posted to the webhook with the type `connected` starts it too.

### Notes

- The socket tries again a minute after it drops, so this fires again after each reconnect.

## Disconnected

- Kind: trigger
- Code: 204
- Type: StreamlabsDisconnected
- Category: Donations & Shops/Streamlabs
- Needs: a Streamlabs socket API token in Donations

Starts its actions when the Streamlabs socket drops.

### Fields

None.

### Arguments

None set.

### When

When the Streamlabs socket closes after it was connected. A failed attempt to connect does not count. A Streamlabs event posted to the webhook with the type `disconnected` starts it too.

### Notes

- Streamerz tries to connect again a minute later.

## Charity Donation

- Kind: trigger
- Code: 205
- Type: StreamlabsCharityDonation
- Category: Donations & Shops/Streamlabs
- Needs: a Streamlabs socket API token in Donations, or Streamlabs events posted to the webhook

Starts its actions for each charity donation from Streamlabs.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Min amount (0: none) | `min` | The least amount to fire for. 0: no minimum. |
| Max amount (0: none) | `max` | With Min at 0 or empty, fires only for exactly this amount. With Min set, the top of the range. 0: no maximum. |

### Arguments

- `charityDonationFrom`: who donated.
- `charityDonationAmount`: the amount, as a number.
- `charityDonationCurrency`: the currency.
- `charityDonationFormattedAmount`: the amount as Streamlabs formats it.
- `charityDonationMessage`: the message with the donation.
- `charityDonationGif`: the GIF sent with the donation.
- `charityDonationIconClassName`: the icon's class name.
- `charityDonationCreatedAt`: when the donation was made.

### When

When Streamlabs sends a charity donation, over the socket or to the webhook (the webhook type `streamlabscharitydonation` or `charity`). Only donations inside the amount range set on the trigger start it.

## Authenticated

- Kind: trigger
- Code: 206
- Type: StreamlabsAuthenticated
- Category: Donations & Shops/Streamlabs
- Needs: a Streamlabs socket API token in Donations

Starts its actions when Streamlabs accepts the socket API token.

### Fields

None.

### Arguments

None set.

### When

When the Streamlabs socket connects. It fires together with Connected, right after it. A Streamlabs event posted to the webhook with the type `authenticated` starts it too.

### Notes

- Streamlabs checks the token before it lets the socket connect, so this is the moment the token is known to work.
