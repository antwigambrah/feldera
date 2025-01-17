// Displays an example value of a value that was generated by a random
// generation method.
//
// Also indicates if the example was modified after generation due to type
// constraints (overflow etc.).

import { getValueFormatter, typeRange } from '$lib/functions/ddl'
import { Field } from '$lib/services/manager'

import { Grid, Typography } from '@mui/material'

export const ExampleDisplay = (props: {
  field: Field
  parsed: boolean | number | string
  example: boolean | number | string | object
}) => {
  const { example, field, parsed } = props
  const toDisplay = getValueFormatter(field.columntype)

  let beforeParsedValue = ''
  const displayParsed = toDisplay(parsed)
  let afterParsedValue = ''

  // Indicates if the value got adjusted by the valueparser due to constraints
  // on the field type
  const adjustments: string[] = []
  if (
    typeof example === 'number' &&
    ['TINYINT', 'SMALLINT', 'INTEGER', 'BIGINT', 'DECIMAL', 'NUMERIC', 'FLOAT', 'DOUBLE'].includes(
      field.columntype.type
    )
  ) {
    const [min, max] = typeRange(field.columntype)
    if (example < min || example > max) {
      adjustments.push('clamp')
    }
  }
  if (typeof example === 'string') {
    beforeParsedValue = "'"
    afterParsedValue = "'"
    if (
      ['VARCHAR', 'CHAR'].includes(field.columntype.type) &&
      field.columntype.precision != null &&
      field.columntype.precision != -1 &&
      example.length > field.columntype.precision
    ) {
      adjustments.push('trimmed')
    } else if (
      ['CHAR'].includes(field.columntype.type) &&
      field.columntype.precision != null &&
      example.length < field.columntype.precision
    ) {
      // HTML has this behaviour that if a string has multiple white-space at
      // the end they get reduced to one. Which is a bit odd for displaying the
      // char. As e.g., 'abc' with char(10) is displayed as 'abc<1space>'
      // instead of 'abc<10space>'
      //
      // TODO: fix with ideas from here:
      // https://stackoverflow.com/questions/433493/why-does-html-require-that-multiple-spaces-show-up-as-a-single-space-in-the-brow
      adjustments.push('padded')
    }
  }

  if (
    typeof example === 'number' &&
    typeof parsed === 'string' &&
    ['DECIMAL'].includes(field.columntype.type) &&
    parsed.length != example.toString().length
  ) {
    adjustments.push('trimmed')
  }

  return (
    <Grid item sm={2} xs={12}>
      <>
        <Typography sx={{ typography: 'subtitle2' }}>
          Example{adjustments.length > 0 ? ': ' + adjustments.join(',') : ':'}
        </Typography>
        <Typography sx={{ typography: 'body2', fontStyle: 'italic' }}>
          {beforeParsedValue}
          {displayParsed}
          {afterParsedValue}
        </Typography>
      </>
    </Grid>
  )
}
