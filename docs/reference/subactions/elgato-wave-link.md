# Elgato / Wave Link (sub-actions)

Actions for Elgato Wave Link, the audio mixer app: mute and volume for inputs and outputs, microphone settings and filters.

## Mute Microphone

- Kind: sub-action
- Code: 14001
- Type: ElgatoWaveLinkMuteMicrophone
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Mutes or unmutes a microphone in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `microphoneName`, `state`.

## Mute Output

- Kind: sub-action
- Code: 14002
- Type: ElgatoWaveLinkMuteOutput
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Mutes or unmutes an output in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `mixerType`, `state`.

## Mute Input

- Kind: sub-action
- Code: 14003
- Type: ElgatoWaveLinkMuteInput
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Mutes or unmutes an input in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `inputName`, `mixerType`, `state`.

## Set Filter Bypass State

- Kind: sub-action
- Code: 14004
- Type: ElgatoWaveLinkSetBypassFilterState
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the bypass state of a filter on an input in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `inputName`, `mixerType`, `state`.

## Set Filter State

- Kind: sub-action
- Code: 14005
- Type: ElgatoWaveLinkSetFilterState
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the state of a filter on an input in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `inputName`, `filterId`, `filterName`, `state`.

## Set Output Monitor Device

- Kind: sub-action
- Code: 14006
- Type: ElgatoWaveLinkSetOutputMonitorDevice
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the device used to monitor an output in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `deviceName`.

## Get Selected Output

- Kind: sub-action
- Code: 14007
- Type: ElgatoWaveLinkGetSelectedOutput
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Reads the selected output in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Set Output Volume

- Kind: sub-action
- Code: 14008
- Type: ElgatoWaveLinkSetOutputVolume
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the volume of an output in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `mixerType`, `volume`, `adjustment`.

## Get Output Volumes

- Kind: sub-action
- Code: 14009
- Type: ElgatoWaveLinkGetOutputVolumes
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Reads the volumes of the outputs in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.

## Set Input Volume

- Kind: sub-action
- Code: 14010
- Type: ElgatoWaveLinkSetInputVolume
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the volume of an input in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `inputName`, `mixerType`, `volume`, `adjustment`.

## Get Input Information

- Kind: sub-action
- Code: 14011
- Type: ElgatoWaveLinkGetInputInformation
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Reads the details of an input in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `inputName`.

## Get Microphone Information

- Kind: sub-action
- Code: 14012
- Type: ElgatoWaveLinkGetMicrophoneInformation
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Reads the details of a microphone in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `microphoneName`.

## Set Microphone Gain

- Kind: sub-action
- Code: 14013
- Type: ElgatoWaveLinkSetMicrophoneGain
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the gain of a microphone in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `microphoneName`, `gain`.

## Set Microphone Output Volume

- Kind: sub-action
- Code: 14014
- Type: ElgatoWaveLinkSetMicrophoneOutputVolume
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the output volume of a microphone in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `microphoneName`, `volume`, `adjustment`.

## Set Microphone Balance

- Kind: sub-action
- Code: 14015
- Type: ElgatoWaveLinkSetMicrophoneBalance
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Sets the balance of a microphone in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `microphoneName`, `balance`.

## Get Filter State

- Kind: sub-action
- Code: 14016
- Type: ElgatoWaveLinkGetFilterState
- Category: Elgato/Wave Link
- Needs: Elgato Wave Link (not wired up yet)

Reads the state of a filter on an input in Elgato Wave Link.

### Fields

None.

### Arguments

None set.

### Notes

- Not done in Streamerz yet: the type is known and kept, but nothing runs it.
- The type keeps these settings: `identifier`, `inputName`, `filterId`, `filterName`.
