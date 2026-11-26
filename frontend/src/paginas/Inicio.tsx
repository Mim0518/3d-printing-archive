import { Badge, Card, Group, List, Stack, Text, Title } from '@mantine/core'
import { useQuery } from '@tanstack/react-query'

import { type Salud, api } from '../api'
import { useYo } from '../sesion'

const FASES = [
  'Base: servidor, base de datos, inicio de sesión y despliegue',
  'Inventario y catálogo (lectura)',
  'Etiquetado y edición del catálogo',
  'Miniaturas y visor 3D',
  'Placas y escenas',
  'Impresoras y materiales',
  'Existencias y costos',
  'Pedidos, clientes y colecciones de venta',
  'Almacenamiento frío (Drive y disco externo)',
  'Bandeja «Por catalogar»',
]

export default function Inicio() {
  const { data: usuario } = useYo()
  const salud = useQuery({ queryKey: ['salud'], queryFn: () => api<Salud>('/salud') })

  return (
    <Stack maw={760}>
      <div>
        <Title order={2}>Hola, {usuario?.nombre}</Title>
        <Text c="dimmed">El catálogo llega en la fase 1. Por ahora está lista la base.</Text>
      </div>
      <Card withBorder>
        <Group justify="space-between">
          <Text fw={500}>Servidor</Text>
          {salud.data ? (
            <Group gap="xs">
              <Badge color={salud.data.estado === 'ok' ? 'green' : 'red'} variant="light">
                {salud.data.estado === 'ok' ? 'en línea' : salud.data.estado}
              </Badge>
              <Text size="sm" c="dimmed">
                versión {salud.data.version}
              </Text>
            </Group>
          ) : (
            <Badge color={salud.isError ? 'red' : 'gray'} variant="light">
              {salud.isError ? 'sin respuesta' : 'revisando…'}
            </Badge>
          )}
        </Group>
      </Card>
      <Card withBorder>
        <Text fw={500} mb="xs">
          Fases
        </Text>
        <List type="ordered" start={0} spacing={4} size="sm">
          {FASES.map((f, i) => (
            <List.Item key={f}>
              <Text span size="sm" c={i === 0 ? undefined : 'dimmed'} fw={i === 0 ? 600 : undefined}>
                {f}
              </Text>
              {i === 0 && (
                <Badge ml="xs" size="xs" variant="light">
                  actual
                </Badge>
              )}
            </List.Item>
          ))}
        </List>
      </Card>
    </Stack>
  )
}
