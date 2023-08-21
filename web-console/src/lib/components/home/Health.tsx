// Should display aggregate health of all pipelines, just a placeholder right
// now.

import { Icon } from '@iconify/react'
import Card from '@mui/material/Card'
import CardContent from '@mui/material/CardContent'
import CardHeader from '@mui/material/CardHeader'
import Divider from '@mui/material/Divider'
import List from '@mui/material/List'
import ListItem from '@mui/material/ListItem'
import ListItemButton from '@mui/material/ListItemButton'
import ListItemIcon from '@mui/material/ListItemIcon'
import ListItemSecondaryAction from '@mui/material/ListItemSecondaryAction'
import ListItemText from '@mui/material/ListItemText'
import Typography from '@mui/material/Typography'

const Health = () => {
  return (
    <Card>
      <CardHeader title='DBSP Health'></CardHeader>

      <CardContent>
        <List component='nav' aria-label='main mailbox'>
          <ListItem disablePadding>
            <ListItemButton>
              <ListItemIcon>
                <Icon icon='bx:error-circle' fontSize={20} />
              </ListItemIcon>
              <ListItemText primary='Reported errors' />
              <ListItemSecondaryAction>
                <Typography variant='h6'>0</Typography>
              </ListItemSecondaryAction>
            </ListItemButton>
          </ListItem>
          <ListItem disablePadding>
            <ListItemButton>
              <ListItemIcon>
                <Icon icon='bx:error-circle' fontSize={20} />
              </ListItemIcon>
              <ListItemText primary='Reported warnings' />
              <ListItemSecondaryAction>
                <Typography variant='h6'>0</Typography>
              </ListItemSecondaryAction>
            </ListItemButton>
          </ListItem>
        </List>
        <Divider sx={{ m: '0 !important' }} />
        <List component='nav' aria-label='secondary mailbox'>
          <ListItem disablePadding>
            <ListItemButton>
              <ListItemIcon>
                <Icon icon='bx:time-five' fontSize={20} />
              </ListItemIcon>
              <ListItemText primary='Other notifications' />
              <ListItemSecondaryAction>
                <Typography variant='h6'>3</Typography>
              </ListItemSecondaryAction>
            </ListItemButton>
          </ListItem>
        </List>
      </CardContent>
    </Card>
  )
}

export default Health