import { Badge, Button, Card, Group, Stack, Text, Tooltip } from '@mantine/core'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from 'react-router'

import { api } from '../api'
import { corto, duracion, maquina, titulo } from '../formato'
import { ESTADO_PLACA } from '../opciones'
import type { PlacaFila } from '../tipos'
import Miniatura from './Miniatura'

/** Las dos últimas carpetas (lo demás se ve al pasar el mouse). */
function cola(carpeta: string): string {
  const p = carpeta.split('/')
  return p.length > 2 ? `…/${p.slice(-2).join('/')}` : carpeta
}

/** Una placa en la lista: vista previa, formatos, laminado y de qué modelos es (con confirmación rápida). */
export default function TarjetaPlaca({ placa: p, sinModelos = false }: { placa: PlacaFila; sinModelos?: boolean }) {
  const qc = useQueryClient()
  const confirmar = useMutation({
    mutationFn: () =>
      api<{ confirmadas: number }>('/placas/confirmar', { method: 'POST', body: JSON.stringify({ placa_ids: [p.id] }) }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['placas'] })
      void qc.invalidateQueries({ queryKey: ['placas-resumen'] })
    },
  })
  const lam = p.laminados[0]
  const sugeridos = p.modelos.filter((m) => m.estado === 'sugerido')
  const confirmados = p.modelos.filter((m) => m.estado === 'confirmado')
  const falta = p.n_presentes < p.n_archivos

  return (
    <Card withBorder padding="xs" component={Link} to={`/placas/${p.id}`} style={{ textDecoration: 'none' }}>
      <Card.Section>
        <Miniatura archivoId={p.portada?.id ?? 0} clave={p.portada?.clave} tamano={512} lado={256} lleno tipo="escena" ext={p.formatos[0]} />
      </Card.Section>
      <Stack gap={4} mt="xs">
        <Group gap={4} wrap="nowrap" justify="space-between" align="flex-start">
          <Text size="sm" fw={500} lineClamp={2} style={{ wordBreak: 'break-word' }}>
            {p.nombre}
          </Text>
          {p.favorita && (
            <Text c="yellow" size="sm" aria-label="favorita">
              ★
            </Text>
          )}
        </Group>
        <Tooltip label={p.carpeta} openDelay={400} multiline maw={360}>
          <Text size="xs" c="dimmed" lineClamp={1}>
            {cola(p.carpeta) || '(raíz)'}
          </Text>
        </Tooltip>
        <Group gap={4}>
          {p.formatos.map((f) => (
            <Badge key={f} size="xs" variant="light" color={['photon', 'pm4n', 'pws', 'ctb', 'pwmo'].includes(f) ? 'teal' : 'cyan'}>
              {f}
            </Badge>
          ))}
          {p.autoguardados > 0 && (
            <Badge size="xs" variant="outline" color="gray">
              {p.autoguardados} {p.autoguardados === 1 ? 'versión' : 'versiones'}
            </Badge>
          )}
          {p.n_placas && p.n_placas > 1 && (
            <Badge size="xs" variant="outline" color="gray">
              {p.n_placas} placas
            </Badge>
          )}
          {p.estado !== 'sin_probar' && (
            <Badge size="xs" color={ESTADO_PLACA[p.estado].color}>
              {ESTADO_PLACA[p.estado].texto}
            </Badge>
          )}
          {falta && (
            <Badge size="xs" color="red" variant="light">
              faltan archivos
            </Badge>
          )}
        </Group>
        {lam && (lam.tiempo_s || lam.volumen_ml || maquina(lam)) && (
          <Text size="xs" c="dimmed">
            {[maquina(lam), duracion(lam.tiempo_s), lam.volumen_ml ? `${corto(lam.volumen_ml)} ml` : '']
              .filter(Boolean)
              .join(' · ')}
          </Text>
        )}
        {!sinModelos && (
          <Group gap={4}>
            {p.sin_modelo && (
              <Badge size="xs" variant="outline" color="gray">
                sin modelo
              </Badge>
            )}
            {confirmados.map((m) => (
              <Badge key={m.id} size="xs" color="green" variant="light">
                {titulo(m)}
                {m.cantidad ? ` ×${m.cantidad}` : ''}
              </Badge>
            ))}
            {sugeridos.map((m) => (
              <Tooltip key={m.id} label={m.origen === 'carpeta' ? 'Sugerido por la carpeta' : 'Sugerido por el nombre'}>
                <Badge size="xs" color="gray" variant="outline">
                  ¿{titulo(m)}?
                </Badge>
              </Tooltip>
            ))}
            {!p.sin_modelo && p.modelos.length === 0 && (
              <Badge size="xs" color="orange" variant="light">
                sin modelo asignado
              </Badge>
            )}
          </Group>
        )}
        {!sinModelos && sugeridos.length > 0 && confirmados.length === 0 && (
          <Button
            size="compact-xs"
            variant="light"
            loading={confirmar.isPending}
            onClick={(e) => {
              e.preventDefault()
              e.stopPropagation()
              confirmar.mutate()
            }}
          >
            Confirmar
          </Button>
        )}
      </Stack>
    </Card>
  )
}
