import { AppShell, Badge, Burger, Button, Group, NavLink, Text, Title } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { Outlet, useLocation, useNavigate } from 'react-router'

import { useLogout, useYo } from '../sesion'

/** Secciones de la especificación; se activan conforme llegan sus fases. */
const SECCIONES = [
  { ruta: '/', nombre: 'Inicio', fase: 0 },
  { ruta: '/catalogo', nombre: 'Catálogo', fase: 1 },
  { ruta: '/placas', nombre: 'Placas y escenas', fase: 4 },
  { ruta: '/impresoras', nombre: 'Impresoras y materiales', fase: 5 },
  { ruta: '/existencias', nombre: 'Existencias', fase: 6 },
  { ruta: '/ventas', nombre: 'Pedidos y colecciones', fase: 7 },
  { ruta: '/frio', nombre: 'Almacenamiento frío', fase: 8 },
  { ruta: '/bandeja', nombre: 'Por catalogar', fase: 9 },
]
const FASE_ACTUAL = 1

export default function Marco() {
  const [abierto, { toggle, close }] = useDisclosure()
  const { data: usuario } = useYo()
  const logout = useLogout()
  const navegar = useNavigate()
  const { pathname } = useLocation()

  return (
    <AppShell
      header={{ height: 56 }}
      navbar={{ width: 250, breakpoint: 'sm', collapsed: { mobile: !abierto } }}
      padding="md"
    >
      <AppShell.Header>
        <Group h="100%" px="md" justify="space-between" wrap="nowrap">
          <Group gap="sm" wrap="nowrap">
            <Burger opened={abierto} onClick={toggle} hiddenFrom="sm" size="sm" aria-label="Menú" />
            <Title order={4}>Archivo 3D</Title>
          </Group>
          <Group gap="sm" wrap="nowrap">
            <Text size="sm" c="dimmed" visibleFrom="xs">
              {usuario?.nombre}
            </Text>
            <Button
              variant="subtle"
              size="xs"
              loading={logout.isPending}
              onClick={() => logout.mutate(undefined, { onSettled: () => navegar('/login') })}
            >
              Salir
            </Button>
          </Group>
        </Group>
      </AppShell.Header>
      <AppShell.Navbar p="xs">
        {SECCIONES.map((s) => {
          const lista = s.fase <= FASE_ACTUAL
          return (
            <NavLink
              key={s.ruta}
              label={s.nombre}
              active={s.ruta === '/' ? pathname === '/' : pathname.startsWith(s.ruta)}
              disabled={!lista}
              rightSection={lista ? null : <Badge size="xs" variant="light" color="gray">fase {s.fase}</Badge>}
              onClick={() => {
                navegar(s.ruta)
                close()
              }}
            />
          )
        })}
      </AppShell.Navbar>
      <AppShell.Main>
        <Outlet />
      </AppShell.Main>
    </AppShell>
  )
}
